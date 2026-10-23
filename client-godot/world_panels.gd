extends Node
const WORLD_PANEL = preload("res://world_panel.gd")
const GIZMO = preload("res://translation_gizmo.gd")
var app
var panels: Dictionary = {}
var shared_gizmos: Dictionary = {}
var sessions: Array = []
var client_id := ""
var capture_id := ""
var keyboard_id := ""
var hover_id := ""
var component_schemas: Array = []
var drive_sources: Dictionary = {}
var windows: Dictionary = {}
var pending_window := ""

func setup(application) -> void:
	app = application
	client_id = "%s-%s" % [Time.get_ticks_usec(), randi()]

func command(body: Dictionary, callback: Callable = Callable()) -> String:
	var key: String = "edit" if body.kind == "edit" else ("request" if body.kind == "edit_session" else "command")
	return app.requests.request(body,callback,[key,"command_id"],"command_id",10000)

func edit(object_id: String, field: String, value: Variant, callback: Callable = Callable(), revision: String = "") -> void:
	var object: Dictionary = app.objects.get(object_id, {})
	if object.is_empty() or not app.editable:
		if callback.is_valid(): callback.call({"ok":false,"message":"Object unavailable or disconnected"})
		return
	var current_field := field_data(object_id, field)
	if current_field.is_empty(): return
	command({"kind":"edit","edit":{"target":app.field_address(object_id,field),"field_id":field,"expected_revision":str(current_field.revision) if revision.is_empty() else revision,"value":value}}, callback)

func field_data(object_id: String, field: String) -> Dictionary:
	for entry in app.objects.get(object_id, {}).get("fields", []):
		if entry.id == field: return entry
	return {}

func session_input(panel_id: String, target_id: String, field: String, action: String, draft: Dictionary, callback: Callable = Callable()) -> void:
	if not app.objects.has(panel_id) or not app.objects.has(target_id):
		if callback.is_valid(): callback.call({"ok":false,"message":"Target unavailable"})
		return
	command({"kind":"edit_session","request":{"client_id":client_id,"context":{"source":app.field_address(panel_id,"inspector_target"),"field_id":"inspector_target"},"target":app.field_address(target_id,field),"field_id":field,"action":action,"draft":draft}}, callback)

func receive(body: Dictionary) -> bool:
	if body.get("kind") == "snapshot":
		component_schemas = body.get("component_schemas", [])
		update_drive_sources(body.get("drive_frame", {}))
	elif body.get("kind") == "drive_frame":
		update_drive_sources(body.frame)
	elif body.get("kind") == "status" and not body.get("editable", false):
		drive_sources.clear()
	if body.get("kind") == "script_document_state":
		for panel in panels.values(): panel.content.script_view.receive(body.document)
		return true
	if body.get("kind") == "edit_sessions":
		sessions = body.get("sessions", [])
		for panel in panels.values(): panel.content.refresh_sessions()
		return true
	return false

func update_drive_sources(frame: Dictionary) -> void:
	drive_sources.clear()
	for object in frame.get("objects", []):
		drive_sources[object.target.instance_id] = object.get("sources", {})

func field_driver(object_id: String, field: String) -> Dictionary:
	var script_id: String = drive_sources.get(object_id, {}).get(field, "")
	for script in app.objects.get(object_id, {}).get("scripts", []):
		if script.id == script_id: return script
	return {}

func open_inspector(target_id: String = "", desktop: bool = false) -> void:
	if not app.editable: return
	var position: Vector3 = app.camera.global_position - app.camera.global_basis.z * 2.1
	var q: Quaternion = app.camera.global_basis.get_rotation_quaternion()
	var target: Variant = app.address_of(app.objects[target_id]) if app.objects.has(target_id) else null
	command({"kind":"structure","command":{"action":"create_inspector","target":target,"position":[position.x,position.y,position.z],"rotation":[q.x,q.y,q.z,q.w]}}, func(result):
		if not result.ok: app.feedback.text = str(result.get("message", "Cannot open inspector"))
		elif desktop: pending_window = result.selected.instance_id)

func close_panel(id: String) -> void:
	if not app.objects.has(id): return
	panels[id].clear_focus()
	if panels[id].content.has_active_edits():
		panels[id].content.close_when_ready = true
		return
	command({"kind":"structure","command":{"action":"delete","target":app.address_of(app.objects[id]),"expected_revision":app.objects[id].object_revision}})

func duplicate_panel(id: String) -> void:
	if not app.objects.has(id): return
	command({"kind":"structure","command":{"action":"duplicate","target":app.address_of(app.objects[id]),"expected_revision":app.objects[id].object_revision}})

func pin_panel(id: String) -> void:
	var object: Dictionary = app.objects.get(id, {})
	if object.is_empty(): return
	if object.panel.get("attachment") != null:
		edit(id, "panel_pin", null)
		return
	var node: Node3D = panels[id]
	var user_basis := Basis(Vector3.UP, app.player_yaw)
	var offset: Vector3 = user_basis.inverse() * (node.global_position - app.player_position)
	var rotation: Quaternion = (user_basis.inverse() * node.global_basis.orthonormalized()).get_rotation_quaternion()
	edit(id, "panel_pin", {"user_id":"$self","offset":[offset.x,offset.y,offset.z],"rotation":[rotation.x,rotation.y,rotation.z,rotation.w]})

func sync() -> void:
	for id in panels.keys():
		if not app.objects.has(id):
			close_window(id)
			panels[id].content.finish_all()
			panels.erase(id)
			if capture_id == id: capture_id = ""
			if keyboard_id == id: keyboard_id = ""
	for id in app.objects:
		if app.objects[id].get("panel") != null and panels.has(id): panels[id].refresh(app.objects[id])
	refresh_gizmos()

func open_window(id: String) -> void:
	if not panels.has(id): return
	app.release_cursor()
	if windows.has(id):
		windows[id].grab_focus()
		return
	var panel = panels[id]
	panel.clear_focus()
	var window := Window.new()
	window.hide()
	window.title = "Kokoverse · Shared inspector"
	window.force_native = true
	window.size = panel.viewport.size
	window.min_size = Vector2i(620, 500)
	window.transient = false
	window.close_requested.connect(close_window.bind(id))
	window.focus_entered.connect(func(): app.release_cursor())
	app.add_child(window)
	var container := SubViewportContainer.new()
	container.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	container.stretch = true
	window.add_child(container)
	panel.viewport.reparent(container)
	panel.viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	windows[id] = window
	window.show()
	window.grab_focus()

func close_window(id: String) -> void:
	if not windows.has(id): return
	var window: Window = windows[id]
	if panels.has(id) and is_instance_valid(panels[id]):
		var panel = panels[id]
		panel.clear_focus()
		panel.viewport.reparent(panel)
		panel.viewport.render_target_update_mode = SubViewport.UPDATE_WHEN_VISIBLE
	windows.erase(id)
	window.hide()
	window.queue_free()

func has_window_focus() -> bool:
	for window in windows.values():
		if window.has_focus(): return true
	return false

func create_surface(id: String):
	var panel = WORLD_PANEL.new()
	app.add_child(panel)
	panel.setup(self, id)
	panels[id] = panel
	return panel

func refresh_gizmos() -> void:
	var wanted := {}
	for object in app.objects.values():
		if object.get("inspector") == null or object.inspector.get("target") == null: continue
		var target_id: String = object.inspector.target.instance_id
		if not app.render_nodes.has(target_id): continue
		wanted[target_id] = true
		if not shared_gizmos.has(target_id):
			var gizmo = GIZMO.new()
			app.add_child(gizmo)
			gizmo.camera = app.camera
			gizmo.shared_display = true
			shared_gizmos[target_id] = gizmo
		shared_gizmos[target_id].set_target(app.render_nodes[target_id].position, false)
		shared_gizmos[target_id].shared_active = app.remote_previews.has(target_id)
		shared_gizmos[target_id].visible = target_id != app.selected_id
	for id in shared_gizmos.keys():
		if not wanted.has(id):
			shared_gizmos[id].queue_free()
			shared_gizmos.erase(id)

func text_focused() -> bool:
	for panel in panels.values():
		if panel.has_text_focus(): return true
	return false

func clear_focus() -> void:
	for panel in panels.values(): panel.clear_focus()
	keyboard_id = ""

func pick(screen: Vector2) -> Dictionary:
	var origin: Vector3 = app.camera.project_ray_origin(screen)
	var direction: Vector3 = app.camera.project_ray_normal(screen)
	var hit: Dictionary = app.get_world_3d().direct_space_state.intersect_ray(PhysicsRayQueryParameters3D.create(origin, origin + direction * 100.0))
	if hit.is_empty() or not hit.collider.has_meta("object_id"): return {}
	var id := str(hit.collider.get_meta("object_id"))
	if not panels.has(id) or direction.dot(panels[id].global_basis.z) >= 0.0: return {}
	return {"id":id,"position":panels[id].canvas_point(hit.position)}

func route_input(event: InputEvent) -> bool:
	if event is InputEventKey and text_focused() and panels.has(keyboard_id):
		if event.keycode == KEY_ESCAPE and event.pressed:
			clear_focus()
			return true
		panels[keyboard_id].viewport.push_input(event.duplicate(), true)
		return true
	if not event is InputEventMouse: return false
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_RIGHT: return false
	if not app.hand_grab.is_empty() or app.gizmo.dragging: return false
	var screen: Vector2 = app.get_viewport().get_visible_rect().size * 0.5 if app.fps_active else event.position
	var hit := pick(screen)
	if capture_id.is_empty() and not app.fps_active and app.get_viewport().gui_get_hovered_control() != null: hit = {}
	if not capture_id.is_empty() and panels.has(capture_id):
		var panel = panels[capture_id]
		var origin: Vector3 = app.camera.project_ray_origin(screen)
		var direction: Vector3 = app.camera.project_ray_normal(screen)
		var point: Variant = Plane(panel.global_basis.z.normalized(), panel.global_position).intersects_ray(origin, direction)
		if point != null: hit = {"id":capture_id,"position":panel.canvas_point(point)}
	if hit.is_empty():
		if not hover_id.is_empty() and panels.has(hover_id): panels[hover_id].leave_pointer()
		hover_id = ""
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT: clear_focus()
		return false
	if hover_id != hit.id and panels.has(hover_id): panels[hover_id].leave_pointer()
	hover_id = hit.id
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		if event.pressed:
			if keyboard_id != hit.id: clear_focus()
			keyboard_id = hit.id
			capture_id = hit.id
			if app.fps_active: app.release_cursor()
		else: capture_id = ""
	panels[hit.id].forward_pointer(event, hit.position)
	# Hovering a panel must not stop FPS mouse-look; clicks and drags belong to the panel.
	return not event is InputEventMouseMotion or not capture_id.is_empty() or not app.fps_active

func tick() -> void:
	if not pending_window.is_empty() and panels.has(pending_window):
		open_window(pending_window)
		pending_window = ""
	for panel in panels.values(): panel.content.tick()
	refresh_gizmos()

func disconnected() -> void:
	sessions.clear()
	clear_focus()
	for panel in panels.values(): panel.content.disconnected()
