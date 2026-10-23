extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func wait_for(test: Callable, message: String) -> bool:
	for _i in range(500):
		await create_timer(0.02).timeout
		if test.call(): return true
	push_error(message)
	return false

func mouse(position: Vector2, pressed: bool) -> void:
	var motion := InputEventMouseMotion.new()
	motion.position = position
	root.push_input(motion, true)
	var event := InputEventMouseButton.new()
	event.button_index = MOUSE_BUTTON_LEFT
	event.position = position
	event.pressed = pressed
	root.push_input(event, true)

func fps_tab(scene, panel, index: int) -> bool:
	var tabs: TabBar = panel.content.tabs
	var point: Vector2 = tabs.get_global_transform() * tabs.get_tab_rect(index).get_center()
	var local := Vector3((point.x/panel.viewport.size.x-0.5)*panel.mesh.size.x,(0.5-point.y/panel.viewport.size.y)*panel.mesh.size.y,0.025)
	var direction: Vector3 = (panel.to_global(local)-scene.camera.global_position).normalized()
	var aim := InputEventMouseMotion.new()
	aim.position = root.size*0.5
	aim.relative = Vector2(wrapf(scene.orbit_yaw-atan2(-direction.x,-direction.z),-PI,PI),scene.orbit_pitch-asin(direction.y))/0.003
	root.push_input(aim,true)
	await process_frame
	await process_frame
	mouse(root.size*0.5,true)
	mouse(root.size*0.5,false)
	if not scene.fps_active or Input.mouse_mode!=Input.MOUSE_MODE_CAPTURED:
		push_error("Inspector tab click released FPS mouse capture"); return false
	return await wait_for(func(): return scene.objects[panel.panel_id].inspector.scripts_open==(index==1),"FPS gaze did not switch the inspector tab")

func run() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await wait_for(func(): return scene.world_panels.panels.size() == 2 and scene.editable, "Shared panels did not appear"): quit(1); return
	scene.set_view_mode(2)
	scene.inspector_shell.hide()
	scene.camera.position = Vector3(0, 1.4, 4.5)
	scene.camera.rotation = Vector3.ZERO
	var ids: Array = scene.world_panels.panels.keys()
	ids.sort()
	var panel = scene.world_panels.panels[ids[0]]
	var other = scene.world_panels.panels[ids[1]]
	scene.world_panels.edit(panel.panel_id, "position", [-0.7,1.4,1])
	scene.world_panels.edit(other.panel_id, "position", [0.7,1.4,1])
	await create_timer(0.3).timeout
	var view = panel.content
	var target_id: String = view.target_id
	var other_target: String = other.content.target_id
	if target_id == other_target: push_error("Fixture targets should differ"); quit(1); return
	var line: LineEdit = view.controls.name[0]
	var canvas_point: Vector2 = line.get_global_rect().get_center()
	var local_point := Vector3((canvas_point.x / panel.viewport.size.x - 0.5)*panel.mesh.size.x, (0.5-canvas_point.y/panel.viewport.size.y)*panel.mesh.size.y, 0.03)
	var screen: Vector2 = scene.camera.unproject_position(panel.to_global(local_point))
	# Exercise actual ray -> panel-local point -> SubViewport -> LineEdit routing.
	mouse(screen, true)
	mouse(screen, false)
	if not await wait_for(func(): return line.has_focus(), "3D pointer did not focus the name field"): quit(1); return
	line.select_all()
	for character in "World panel edit ":
		var key := InputEventKey.new()
		key.keycode = character.unicode_at(0)
		key.unicode = character.unicode_at(0)
		key.pressed = true
		root.push_input(key, true)
		key = key.duplicate()
		key.pressed = false
		root.push_input(key, true)
	if not await wait_for(func(): return scene.world_panels.field_data(target_id,"name").get("value") == "World panel edit ", "World panel typing was not committed"): quit(1); return
	if not scene.grounded: push_error("Typing Space made the player jump"); quit(1); return
	if other.content.target_id != other_target: push_error("Editing switched another panel target"); quit(1); return
	if not scene.world_panels.shared_gizmos.has(other_target): push_error("Shared target gizmo is missing"); quit(1); return
	scene.world_panels.clear_focus()
	if not await wait_for(func(): return view.edits.is_empty(), "Editing lease did not release"): quit(1); return
	scene.world_panels.edit(panel.panel_id, "panel_size", [720,640])
	if not await wait_for(func(): return panel.viewport.size.y == 640 and view.scroll.get_v_scroll_bar().max_value > view.scroll.get_v_scroll_bar().page + 10, "Resized panel did not scroll"): quit(1); return
	var bar: VScrollBar = view.scroll.get_v_scroll_bar()
	view.scroll.scroll_vertical = int(maxf(0,bar.max_value-bar.page)*0.5)
	if not await wait_for(func(): return scene.objects[panel.panel_id].inspector.scroll[1] > 0.1, "Scroll was not shared"): quit(1); return
	# Read-only shared handles follow the same preview as the rendered object.
	var object: Dictionary = scene.objects[other_target]
	var preview := {"target":object.address,"expected_revision":scene.world_panels.field_data(other_target,"position").revision,"position":[2.0,1.0,0.0]}
	scene.set_remote_previews([{"owner":"test-remote","preview":preview}])
	await create_timer(0.15).timeout
	if scene.world_panels.shared_gizmos[other_target].position.distance_to(scene.render_nodes[other_target].position)>0.01:
		push_error("Shared gizmo did not follow the remote preview"); quit(1); return
	scene.set_remote_previews([])
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--screenshot="):
			for node in scene.get_children():
				if node is CanvasLayer: node.hide()
			scene.camera.position = Vector3(0,1.4,3.2)
			await process_frame
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(argument.trim_prefix("--screenshot="))
	# The panel must use the same Rust grab/rotate/scale path as images and boxes.
	scene.set_view_mode(1)
	scene.orbit_yaw = 0.0
	scene.orbit_pitch = 0.0
	var placement := Vector3(scene.player_position.x, scene.player_position.y + 1.15, scene.player_position.z - 1.5)
	scene.world_panels.edit(panel.panel_id, "position", [placement.x,placement.y,placement.z])
	if not await wait_for(func(): return panel.position.distance_to(placement)<0.01, "Could not place panel for grab"): quit(1); return
	await create_timer(0.1).timeout
	if not await fps_tab(scene,panel,1): quit(1); return
	if not await fps_tab(scene,panel,0): quit(1); return
	scene.orbit_yaw = 0.0
	scene.orbit_pitch = 0.0
	await create_timer(0.1).timeout
	var grab := InputEventMouseButton.new()
	grab.button_index = MOUSE_BUTTON_RIGHT
	grab.pressed = true
	grab.position = root.get_visible_rect().size * 0.5
	root.push_input(grab, true)
	if not await wait_for(func(): return not scene.hand_grab.is_empty() and scene.hand_grab.preview.target.instance_id == panel.panel_id, "RMB did not grab the panel"): quit(1); return
	scene.queue_message({"kind":"grab","action":"scale","factor":1.2})
	scene.queue_message({"kind":"grab","action":"rotate","delta":[0.2,0.1],"vertical":false})
	if not await wait_for(func(): return scene.hand_grab.preview.scale[0] > 1.1, "Panel scale preview missing"): quit(1); return
	grab = grab.duplicate()
	grab.pressed = false
	root.push_input(grab, true)
	if not await wait_for(func(): return not scene.pending and scene.hand_grab.is_empty() and scene.objects[panel.panel_id].world_scale[0] > 1.1, "Panel grab did not commit"): quit(1); return
	print("WORLD PANELS SMOKE PASS: 3D pointer, FPS gaze tabs, text entry, independent targets, shared scroll and gizmos")
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
