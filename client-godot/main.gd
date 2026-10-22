extends Node3D

const MAX_FRAME := 1048576
const AVATAR = preload("res://avatar.gd")
const TRANSLATION_GIZMO = preload("res://translation_gizmo.gd")
const ASSET_IMPORTER = preload("res://asset_importer.gd")
const SCRIPT_EDITOR = preload("res://script_editor.gd")
var script_editor = SCRIPT_EDITOR.new()
var script_templates: Array = []
var script_statuses: Array = []
var world_tree_signature := ""
var asset_importer = ASSET_IMPORTER.new()
var video_controls: VBoxContainer
var avatar_button: Button
var reset_avatar: Button
var video_seek: HSlider
var video_time: Label
var video_loop: CheckBox
var seeking_video := false
var clock_ready := false
var clock_offset := 0.0
var clock_best_rtt := INF
var clock_next_ping := 0.0
var clock_requests: Dictionary = {}
var tcp := StreamPeerTCP.new()
var received := PackedByteArray()
var outgoing := PackedByteArray()
var port := 47000
var session := "demo"
var epoch := ""
var sequence := 0
var retry_at := 0.0
var connected := false
var editable := false
var current: Dictionary = {}
var fields: Dictionary = {}
var dirty: Dictionary = {}
var pending := false
var pending_action := ""
var pending_edit: Dictionary = {}
var inspector_edits: Dictionary = {}
var inspector_shell: Control
var inspector_toggle: Button
var filling := false
var objects: Dictionary = {}
var render_nodes: Dictionary = {}
var selected_id := ""
var tree_objects: VBoxContainer
var object_count: Label
var create_button: Button
var image_button: Button
var image_textures: Dictionary = {}
var duplicate_button: Button
var delete_button: Button
var selection_outline := MeshInstance3D.new()
var gizmo = TRANSLATION_GIZMO.new()
var gizmo_target := ""
var gizmo_revision := ""
var gizmo_preview := Vector3.ZERO
var gizmo_preview_active := false
var remote_previews: Dictionary = {}
var last_preview_at := 0
var camera := Camera3D.new()
var cube: MeshInstance3D
var material: StandardMaterial3D
var avatars: Dictionary = {}
var self_id := "local"
var player_position := Vector3(0, 0, 3)
var player_target_position := Vector3(0, 0, 3)
var player_yaw := 0.0
var player_initialized := false
var avatar_camera := true
var first_person := false
var fps_active := false
var hand_grab: Dictionary = {}
var grab_waiting := false
var grab_releasing := false
var right_down := false
var crosshair: Label
var interaction_rays: Array = []
var aiming_at_collider := false
var laser_mesh := ImmediateMesh.new()
var grounded := true
var jump_requested := false
var view_mode: OptionButton
var orbit_yaw := 0.45
var orbit_pitch := -0.25
var orbit_distance := 6.0
var last_input_at := 0
var people_label: Label
var nickname: LineEdit
var status_label: Label
var object_label: Label
var revision_label: Label
var feedback: Label
var name_input: LineEdit
var position_inputs: Array[SpinBox] = []
var rotation_inputs: Array[SpinBox] = []
var scale_inputs: Array[SpinBox] = []
var collider_inputs: Array[SpinBox] = []
var collider_enabled: CheckBox
var collider_solid: CheckBox
var color_input: ColorPickerButton
var apply_buttons: Array[Button] = []
var look := false
var capture_path := ""
var frames := 0
var avatar_restore_started := false

func _ready() -> void:
	var bindings := {"move_left":KEY_A, "move_right":KEY_D, "move_forward":KEY_W, "move_back":KEY_S, "jump":KEY_SPACE, "grab_rotate":KEY_E}
	for action in bindings:
		if not InputMap.has_action(action):
			InputMap.add_action(action)
			var key := InputEventKey.new()
			key.physical_keycode = bindings[action]
			InputMap.action_add_event(action, key)
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--port="): port = int(argument.trim_prefix("--port="))
		if argument.begins_with("--session="): session = argument.trim_prefix("--session=")
		if argument.begins_with("--capture="): capture_path = argument.trim_prefix("--capture=")
	build_world()
	build_ui()
	add_child(asset_importer)
	asset_importer.configure(self)
	connect_engine()

func build_world() -> void:
	var environment := WorldEnvironment.new()
	var settings := Environment.new()
	settings.background_mode = Environment.BG_COLOR
	settings.background_color = Color("243645")
	settings.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	settings.ambient_light_color = Color("b7d3e0")
	settings.ambient_light_energy = 0.65
	environment.environment = settings
	add_child(environment)
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-55, -30, 0)
	sun.light_energy = 1.8
	sun.shadow_enabled = true
	add_child(sun)
	var floor_mesh := MeshInstance3D.new()
	var plane := PlaneMesh.new()
	plane.size = Vector2(100, 100)
	floor_mesh.mesh = plane
	var floor_material := StandardMaterial3D.new()
	floor_material.albedo_color = Color("344953")
	floor_material.roughness = 0.95
	floor_mesh.material_override = floor_material
	add_child(floor_mesh)
	var grid := ImmediateMesh.new()
	grid.surface_begin(Mesh.PRIMITIVE_LINES)
	for n in range(-20, 21):
		grid.surface_add_vertex(Vector3(n, 0.008, -20))
		grid.surface_add_vertex(Vector3(n, 0.008, 20))
		grid.surface_add_vertex(Vector3(-20, 0.008, n))
		grid.surface_add_vertex(Vector3(20, 0.008, n))
	grid.surface_end()
	var grid_node := MeshInstance3D.new()
	grid_node.mesh = grid
	var grid_material := StandardMaterial3D.new()
	grid_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	grid_material.albedo_color = Color("46616a")
	grid_node.material_override = grid_material
	add_child(grid_node)
	var outline := ImmediateMesh.new()
	outline.surface_begin(Mesh.PRIMITIVE_LINES)
	var corners: Array[Vector3] = []
	for x in [-0.78, 0.78]:
		for y in [-0.78, 0.78]:
			for z in [-0.78, 0.78]: corners.append(Vector3(x, y, z))
	for i in range(8):
		for bit in [1, 2, 4]:
			if (i ^ bit) > i:
				outline.surface_add_vertex(corners[i])
				outline.surface_add_vertex(corners[i ^ bit])
	outline.surface_end()
	selection_outline.mesh = outline
	var outline_material := StandardMaterial3D.new()
	outline_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	outline_material.albedo_color = Color("a2ece7")
	selection_outline.material_override = outline_material
	selection_outline.visible = false
	add_child(selection_outline)
	camera.position = Vector3(5, 4, 8)
	add_child(camera)
	camera.look_at(Vector3(0, 0.6, 0))
	camera.current = true
	add_child(gizmo)
	var laser_display := MeshInstance3D.new()
	laser_display.mesh = laser_mesh
	var laser_material := StandardMaterial3D.new()
	laser_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	laser_material.vertex_color_use_as_albedo = true
	laser_display.material_override = laser_material
	add_child(laser_display)
	gizmo.camera = camera
	gizmo.drag_started.connect(start_gizmo_drag)
	gizmo.preview_changed.connect(preview_gizmo_drag)
	gizmo.drag_finished.connect(finish_gizmo_drag)
	gizmo.drag_cancelled.connect(cancel_gizmo_drag)

func label(text: String, size: int = 15, color: Color = Color("d8e6ec")) -> Label:
	var item := Label.new()
	item.text = text
	item.add_theme_font_size_override("font_size", size)
	item.add_theme_color_override("font_color", color)
	return item

func panel(parent: Node, left: float, top: float, right: float, bottom: float) -> VBoxContainer:
	var shell := PanelContainer.new()
	shell.offset_left = left
	shell.offset_top = top
	shell.offset_right = right
	shell.offset_bottom = bottom
	var style := StyleBoxFlat.new()
	style.bg_color = Color("172630")
	style.border_color = Color("3a505c")
	style.set_border_width_all(1)
	style.set_corner_radius_all(10)
	style.content_margin_left = 20
	style.content_margin_right = 20
	style.content_margin_top = 18
	style.content_margin_bottom = 18
	shell.add_theme_stylebox_override("panel", style)
	parent.add_child(shell)
	var stack := VBoxContainer.new()
	stack.add_theme_constant_override("separation", 14)
	shell.add_child(stack)
	return stack

func build_ui() -> void:
	var layer := CanvasLayer.new()
	add_child(layer)
	var theme := Theme.new()
	theme.default_font_size = 15
	var root := Control.new()
	root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.theme = theme
	layer.add_child(root)
	crosshair = label("+", 24, Color("a2ece7"))
	crosshair.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	crosshair.position = Vector2(-8,-16)
	crosshair.mouse_filter = Control.MOUSE_FILTER_IGNORE
	crosshair.visible = false
	root.add_child(crosshair)
	var header := panel(root, 24, 22, 1256, 92)
	header.get_parent().set_anchors_preset(Control.PRESET_TOP_WIDE)
	header.get_parent().offset_right = -24
	var row := HBoxContainer.new()
	header.add_child(row)
	row.add_child(label("kokoverse", 25))
	row.add_child(label("  /  Workshop", 17, Color("9cb5c2")))
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(spacer)
	view_mode = OptionButton.new()
	view_mode.add_item("Third person")
	view_mode.add_item("First person / FPS")
	view_mode.add_item("Fly camera")
	view_mode.item_selected.connect(set_view_mode)
	row.add_child(view_mode)
	inspector_toggle = Button.new()
	inspector_toggle.text = "Inspector [I]"
	inspector_toggle.toggle_mode = true
	inspector_toggle.button_pressed = true
	inspector_toggle.toggled.connect(func(visible):
		inspector_shell.visible = visible
		inspector_toggle.release_focus())
	row.add_child(inspector_toggle)
	status_label = label("Connecting…", 14, Color("6ed3dc"))
	row.add_child(status_label)
	var tree := panel(root, 24, 112, 264, 510)
	tree.add_child(label("WORLD", 12, Color("8ca7b6")))
	tree.add_child(label("Workshop / " + session, 16))
	tree.add_child(label("Drop images, models or videos here", 12, Color("8ca7b6")))
	create_button = Button.new()
	create_button.text = "+ Cube"
	create_button.custom_minimum_size.y = 36
	create_button.pressed.connect(func(): submit_structure("create"))
	var creation := HBoxContainer.new()
	tree.add_child(creation)
	create_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	creation.add_child(create_button)
	image_button = Button.new()
	image_button.text = "Import…"
	image_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	image_button.pressed.connect(choose_image)
	creation.add_child(image_button)
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size.y = 180
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	tree.add_child(scroll)
	tree_objects = VBoxContainer.new()
	tree_objects.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(tree_objects)
	var tools := HBoxContainer.new()
	tree.add_child(tools)
	duplicate_button = Button.new()
	duplicate_button.text = "Duplicate"
	duplicate_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	duplicate_button.pressed.connect(func(): submit_structure("duplicate"))
	tools.add_child(duplicate_button)
	delete_button = Button.new()
	delete_button.text = "Delete…"
	delete_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	delete_button.pressed.connect(confirm_delete)
	tools.add_child(delete_button)
	object_count = label("Waiting for world…", 12, Color("8ca7b6"))
	tree.add_child(object_count)
	var people := panel(root, 24, 530, 264, 700)
	people.add_child(label("PEOPLE", 12, Color("8ca7b6")))
	nickname = LineEdit.new()
	nickname.placeholder_text = "Your display name"
	nickname.max_length = 40
	nickname.text_submitted.connect(func(text): queue_message({"kind":"profile", "name":text}); nickname.release_focus())
	people.add_child(nickname)
	var avatar_actions := HBoxContainer.new()
	people.add_child(avatar_actions)
	avatar_button = Button.new()
	avatar_button.text = "Avatar texture…"
	avatar_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	avatar_button.tooltip_text = "Choose a PNG, JPEG or WebP texture for your Oekaki avatar"
	avatar_button.pressed.connect(func(): asset_importer.choose_avatar_texture())
	avatar_actions.add_child(avatar_button)
	reset_avatar = Button.new()
	reset_avatar.text = "Reset"
	reset_avatar.pressed.connect(func(): asset_importer.reset_avatar_texture())
	avatar_actions.add_child(reset_avatar)
	people_label = label("Connecting…", 13, Color("9cb5c2"))
	people_label.max_lines_visible = 4
	people.add_child(people_label)
	var inspector_panel := panel(root, -340, 112, -24, 708)
	inspector_shell = inspector_panel.get_parent()
	inspector_panel.get_parent().anchor_left = 1.0
	inspector_panel.get_parent().anchor_right = 1.0
	var inspector_scroll := ScrollContainer.new()
	inspector_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	inspector_panel.add_child(inspector_scroll)
	var inspector := VBoxContainer.new()
	inspector.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	inspector_scroll.add_child(inspector)
	inspector.add_child(label("INSPECTOR", 12, Color("8ca7b6")))
	object_label = label("Shared cube", 22)
	inspector.add_child(object_label)
	revision_label = label("Waiting for world…", 12, Color("8ca7b6"))
	inspector.add_child(revision_label)
	var scripts_button := Button.new()
	scripts_button.text = "Script Systems…"
	scripts_button.pressed.connect(func():
		if current.is_empty(): return
		release_cursor()
		script_editor.open(selected_id))
	inspector.add_child(scripts_button)
	inspector.add_child(HSeparator.new())
	inspector.add_child(label("Slot · Name", 14))
	name_input = LineEdit.new()
	name_input.max_length = 80
	name_input.custom_minimum_size.y = 36
	inspector.add_child(name_input)
	name_input.text_changed.connect(func(_v): queue_inspector_edit("name"))
	name_input.text_submitted.connect(func(_v): submit("name", name_input.text))
	add_apply(inspector, "Rename", func(): submit("name", name_input.text))
	inspector.add_child(label("Transform · Position", 14))
	var axes := HBoxContainer.new()
	inspector.add_child(axes)
	for axis in ["X", "Y", "Z"]:
		var column := VBoxContainer.new()
		column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		axes.add_child(column)
		column.add_child(label(axis, 12, Color("8ca7b6")))
		var spin := SpinBox.new()
		spin.min_value = -1000
		spin.max_value = 1000
		spin.step = 0.01
		spin.custom_minimum_size.x = 80
		spin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		column.add_child(spin)
		position_inputs.append(spin)
		spin.get_line_edit().text_changed.connect(func(_v): queue_inspector_edit("position"))
		spin.value_changed.connect(func(value): queue_inspector_edit("position",spin,value))
	add_apply(inspector, "Move", func():
		for spin in position_inputs: spin.apply()
		submit("position", [position_inputs[0].value, position_inputs[1].value, position_inputs[2].value]))
	inspector.add_child(label("MeshRenderer · Color", 14))
	color_input = ColorPickerButton.new()
	color_input.custom_minimum_size.y = 40
	color_input.edit_alpha = false
	inspector.add_child(color_input)
	color_input.color_changed.connect(func(_v): queue_inspector_edit("color"))
	add_apply(inspector, "Apply color", func():
		var c := color_input.color
		submit("color", [c.r, c.g, c.b, c.a]))
	rotation_inputs = add_vector_editor(inspector, "Rotation · degrees", "rotation", -36000, 36000, 1)
	add_apply(inspector, "Apply rotation", func():
		for spin in rotation_inputs: spin.apply()
		var q := Quaternion.from_euler(Vector3(deg_to_rad(rotation_inputs[0].value),deg_to_rad(rotation_inputs[1].value),deg_to_rad(rotation_inputs[2].value)))
		submit("rotation",[q.x,q.y,q.z,q.w]))
	scale_inputs = add_vector_editor(inspector, "Scale", "scale", 0.05, 20, 0.05)
	add_apply(inspector, "Apply scale", func():
		for spin in scale_inputs: spin.apply()
		submit("scale",[scale_inputs[0].value,scale_inputs[1].value,scale_inputs[2].value]))
	collider_enabled = CheckBox.new()
	collider_enabled.text = "Collider · enabled / grabbable"
	inspector.add_child(collider_enabled)
	collider_enabled.toggled.connect(func(_v): queue_inspector_edit("collider"))
	collider_solid = CheckBox.new()
	collider_solid.text = "Solid · blocks the character"
	inspector.add_child(collider_solid)
	collider_solid.toggled.connect(func(_v): queue_inspector_edit("collider"))
	collider_inputs = add_vector_editor(inspector, "Collider · half extents", "collider", 0.001, 50, 0.01)
	add_apply(inspector, "Apply collider", func():
		for spin in collider_inputs: spin.apply()
		var shape: Variant = {"half_extents":[collider_inputs[0].value,collider_inputs[1].value,collider_inputs[2].value],"solid":collider_solid.button_pressed} if collider_enabled.button_pressed else null
		submit("collider",shape))
	feedback = label("Changes sync automatically.", 13, Color("9cb5c2"))
	feedback.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	inspector.add_child(feedback)
	video_controls = VBoxContainer.new()
	video_controls.add_child(label("Video playback · shared",12))
	var play_button := Button.new()
	play_button.text = "Play / Pause"
	play_button.pressed.connect(func(): asset_importer.toggle_video())
	video_controls.add_child(play_button)
	var restart_button := Button.new()
	restart_button.text = "Restart"
	restart_button.pressed.connect(func(): asset_importer.restart_video())
	video_controls.add_child(restart_button)
	apply_buttons.append(play_button)
	apply_buttons.append(restart_button)
	video_time = label("Waiting for host clock…",12)
	video_controls.add_child(video_time)
	video_seek = HSlider.new()
	video_seek.step = 0.1
	video_seek.drag_started.connect(func(): seeking_video = true)
	video_seek.drag_ended.connect(func(changed: bool):
		seeking_video = false
		if changed: send_playback("seek",{"position_seconds":video_seek.value})
	)
	video_controls.add_child(video_seek)
	video_loop = CheckBox.new()
	video_loop.text = "Loop"
	video_loop.toggled.connect(func(enabled: bool):
		if not filling: send_playback("loop",{"looping":enabled})
	)
	video_controls.add_child(video_loop)
	inspector.add_child(video_controls)
	inspector.move_child(video_controls,4)
	video_controls.visible = false
	var help := panel(root, 24, -82, 650, -22)
	help.get_parent().anchor_top = 1.0
	help.get_parent().anchor_bottom = 1.0
	help.add_child(label("WASD Move · Space Jump · V View · RMB Hold · E Rotate\nWheel Distance · Shift+Wheel Scale · Shift+E Yaw · LMB Straighten · Esc Cursor", 12))
	script_editor.setup(self)

func add_vector_editor(parent: Node, title: String, field_id: String, minimum: float, maximum: float, step: float) -> Array[SpinBox]:
	parent.add_child(label(title,14))
	var row := HBoxContainer.new()
	parent.add_child(row)
	var inputs: Array[SpinBox] = []
	for axis in ["X","Y","Z"]:
		var column := VBoxContainer.new()
		column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		row.add_child(column)
		column.add_child(label(axis,12,Color("8ca7b6")))
		var spin := SpinBox.new()
		spin.min_value = minimum
		spin.max_value = maximum
		spin.step = step
		spin.custom_minimum_size.x = 75
		column.add_child(spin)
		spin.value_changed.connect(func(value): queue_inspector_edit(field_id,spin,value))
		spin.get_line_edit().text_changed.connect(func(_v): queue_inspector_edit(field_id))
		inputs.append(spin)
	return inputs

func add_apply(parent: Node, text: String, action: Callable) -> void:
	var button := Button.new()
	button.text = text
	button.custom_minimum_size.y = 32
	button.pressed.connect(action)
	parent.add_child(button)
	button.visible = false
	apply_buttons.append(button)

func mark_dirty(id: String) -> void:
	if filling or not fields.has(id): return
	if not dirty.has(id): dirty[id] = fields[id].revision

func inspector_key(target: Dictionary, field_id: String) -> String:
	return target.instance_id + ":" + field_id

func inspector_value(id: String, override_spin: SpinBox = null, override_value: float = 0.0) -> Variant:
	if id == "name": return name_input.text
	if id == "color":
		var c := color_input.color
		return [c.r,c.g,c.b,c.a]
	if id == "collider" and not collider_enabled.button_pressed: return null
	var inputs: Array[SpinBox] = position_inputs
	if id == "rotation": inputs = rotation_inputs
	if id == "scale": inputs = scale_inputs
	if id == "collider": inputs = collider_inputs
	var values := []
	for spin in inputs:
		if spin == override_spin:
			values.append(override_value)
			continue
		var raw := spin.get_line_edit().text.strip_edges()
		if not raw.is_valid_float(): return null
		values.append(clampf(raw.to_float(),spin.min_value,spin.max_value))
	if id == "rotation":
		var q := Quaternion.from_euler(Vector3(deg_to_rad(values[0]),deg_to_rad(values[1]),deg_to_rad(values[2])))
		return [q.x,q.y,q.z,q.w]
	if id == "collider": return {"half_extents":values,"solid":collider_solid.button_pressed}
	return values

func queue_inspector_edit(id: String, override_spin: SpinBox = null, override_value: float = 0.0) -> void:
	if filling or current.is_empty() or not editable: return
	mark_dirty(id)
	var key := inspector_key(current.address,id)
	var value: Variant = inspector_value(id,override_spin,override_value)
	if (value == null and not (id == "collider" and not collider_enabled.button_pressed)) or (id == "name" and str(value).strip_edges().is_empty()):
		inspector_edits.erase(key)
		return
	var due: int = inspector_edits.get(key,{}).get("due",Time.get_ticks_msec()+160)
	inspector_edits[key] = {"target":current.address.duplicate(),"field_id":id,"expected_revision":dirty[id],"value":value,"due":due}

func flush_inspector_edits() -> void:
	if not editable or pending or grab_waiting or not hand_grab.is_empty() or gizmo_preview_active: return
	for key in inspector_edits.keys():
		var edit: Dictionary = inspector_edits[key]
		if Time.get_ticks_msec() < edit.due: continue
		inspector_edits.erase(key)
		send_field_edit(edit)
		return

func send_field_edit(edit: Dictionary) -> void:
	pending = true
	pending_action = "edit"
	pending_edit = edit.duplicate(true)
	feedback.text = "Syncing…"
	var command := edit.duplicate(true)
	command.erase("due")
	command.target = {"instance_id":edit.target.instance_id,"local_id":int(edit.target.local_id)}
	command.command_id = str(Time.get_ticks_usec())
	queue_message({"kind":"edit","edit":command})

func submit(id: String, value: Variant) -> void:
	if not editable or pending or current.is_empty(): return
	inspector_edits.erase(inspector_key(current.address,id))
	send_field_edit({"target":current.address.duplicate(),"field_id":id,"expected_revision":dirty.get(id,fields[id].revision),"value":value})

func start_gizmo_drag() -> void:
	gizmo_target = selected_id
	gizmo_revision = fields.position.revision
	dirty["position"] = gizmo_revision
	gizmo_preview = cube.position
	gizmo_preview_active = true
	send_gizmo_preview()
	feedback.text = "Drag to move · Shift: 0.25 snap · Esc: cancel"

func preview_gizmo_drag(position: Vector3) -> void:
	gizmo_preview = position
	if not render_nodes.has(gizmo_target): return
	render_nodes[gizmo_target].position = position
	selection_outline.position = position
	filling = true
	for i in range(3): position_inputs[i].value = position[i]
	filling = false

func restore_gizmo_visual() -> void:
	if gizmo_preview_active and connected: queue_message({"kind":"drag_preview", "preview":null})
	if objects.has(gizmo_target) and render_nodes.has(gizmo_target):
		var p: Array = objects[gizmo_target].world_position
		render_nodes[gizmo_target].position = Vector3(p[0], p[1], p[2])
	gizmo_preview_active = false
	gizmo_target = ""
	update_selected()

func send_gizmo_preview() -> void:
	if not connected or not gizmo_preview_active or not objects.has(gizmo_target): return
	queue_message({"kind":"drag_preview", "preview":{
		"target":address_of(objects[gizmo_target]), "expected_revision":gizmo_revision,
		"position":[gizmo_preview.x, gizmo_preview.y, gizmo_preview.z]}})

func valid_remote_preview(preview: Dictionary) -> bool:
	var id: String = preview.target.instance_id
	if not objects.has(id) or objects[id].address != preview.target: return false
	var revisions := {}
	for field in objects[id].fields: revisions[field.id] = field.revision
	return revisions.get("position") == preview.expected_revision and (preview.get("expected_rotation_revision") == null or revisions.get("rotation") == preview.expected_rotation_revision) and (preview.get("expected_scale_revision") == null or revisions.get("scale") == preview.expected_scale_revision)

func object_quaternion(object: Dictionary) -> Quaternion:
	var q: Array = object.get("world_rotation",[0,0,0,1])
	return Quaternion(q[0],q[1],q[2],q[3])

func apply_object_transform(node: Node3D, object: Dictionary) -> void:
	var p: Array = object.world_position
	var s: Array = object.get("world_scale",[1,1,1])
	node.position = Vector3(p[0],p[1],p[2])
	node.quaternion = object_quaternion(object)
	node.scale = Vector3(s[0],s[1],s[2])

func update_selection_outline() -> void:
	if current.is_empty(): return
	selection_outline.transform = cube.transform
	if cube.mesh is QuadMesh: selection_outline.scale *= Vector3(cube.mesh.size.x / 1.5, cube.mesh.size.y / 1.5, 0.06)
	if current.get("visual",{}).get("kind","")=="model":
		var half: Array = current.visual.half_extents
		selection_outline.scale *= Vector3(half[0],half[1],half[2])/0.75

func set_remote_previews(entries: Array) -> void:
	var next := {}
	# Engine orders owners consistently when multiple people preview the same object.
	for entry in entries:
		var preview: Dictionary = entry.preview
		if valid_remote_preview(preview) and not next.has(preview.target.instance_id):
			next[preview.target.instance_id] = preview
	for id in remote_previews:
		if not next.has(id) and render_nodes.has(id) and objects.has(id):
			if gizmo_preview_active and gizmo_target == id: continue
			apply_object_transform(render_nodes[id],objects[id])
	remote_previews = next

func cancel_gizmo_drag() -> void:
	dirty.erase("position")
	restore_gizmo_visual()
	feedback.text = "Move cancelled."

func finish_gizmo_drag(position: Vector3) -> void:
	if position.distance_squared_to(gizmo.drag_origin) < 0.000001:
		cancel_gizmo_drag()
		return
	submit("position", [position.x, position.y, position.z])
	if pending: pending_action = "gizmo"
	else: cancel_gizmo_drag()

func address_of(object: Dictionary) -> Dictionary:
	return {"instance_id":object.address.instance_id, "local_id":int(object.address.local_id)}

func submit_structure(action: String, target: Dictionary = {}) -> void:
	if not editable or pending: return
	if action != "create" and current.is_empty() and target.is_empty(): return
	var command := {"command_id":str(Time.get_ticks_usec()), "action":action}
	if action != "create":
		var object := current if target.is_empty() else target
		command.target = address_of(object)
		command.expected_revision = object.object_revision
	pending = true
	pending_action = action
	feedback.text = "Applying…"
	queue_message({"kind":"structure", "command":command})

func confirm_delete() -> void:
	if current.is_empty(): return
	var target: Dictionary = current.duplicate(true)
	var dialog := ConfirmationDialog.new()
	dialog.title = "Delete object"
	dialog.dialog_text = "Delete “%s” for everyone in this world?" % fields.name.value
	dialog.ok_button_text = "Delete"
	dialog.confirmed.connect(func(): submit_structure("delete", target); dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free)
	add_child(dialog)
	dialog.popup_centered(Vector2i(420, 140))

func choose_image() -> void:
	asset_importer.choose_files()

func import_image(path: String) -> void:
	if not editable or pending: return
	var image := Image.load_from_file(path)
	if image == null or image.is_empty():
		feedback.text = "Could not read that image. Choose PNG or JPEG."
		return
	var largest := maxi(image.get_width(), image.get_height())
	if largest > 1024:
		var factor := 1024.0 / largest
		image.resize(maxi(1, int(image.get_width() * factor)), maxi(1, int(image.get_height() * factor)), Image.INTERPOLATE_LANCZOS)
	var data := image.save_png_to_buffer() if image.detect_alpha() else image.save_jpg_to_buffer(0.85)
	var prefix := "data:image/png;base64," if image.detect_alpha() else "data:image/jpeg;base64,"
	while data.size() > 192 * 1024 and maxi(image.get_width(), image.get_height()) > 64:
		image.resize(maxi(1, int(image.get_width() * 0.75)), maxi(1, int(image.get_height() * 0.75)), Image.INTERPOLATE_LANCZOS)
		data = image.save_png_to_buffer() if image.detect_alpha() else image.save_jpg_to_buffer(0.85)
	if data.size() > 192 * 1024:
		feedback.text = "Image is too large. Try a smaller image."
		return
	pending = true
	pending_action = "create_image"
	feedback.text = "Sharing image…"
	queue_message({"kind":"structure", "command":{"command_id":str(Time.get_ticks_usec()), "action":"create_image", "data_url":prefix + Marshalls.raw_to_base64(data)}})

func queue_message(body: Dictionary) -> void:
	sequence += 1
	var data := JSON.stringify({"protocol_version":2, "session_id":session,
		"authority_epoch":epoch, "sequence":str(sequence), "body":body}).to_utf8_buffer()
	var frame := PackedByteArray()
	frame.resize(4)
	frame[0] = (data.size() >> 24) & 255
	frame[1] = (data.size() >> 16) & 255
	frame[2] = (data.size() >> 8) & 255
	frame[3] = data.size() & 255
	frame.append_array(data)
	if outgoing.size() + frame.size() <= MAX_FRAME: outgoing.append_array(frame)
	else: disconnect_engine("Send queue full. Reconnecting…")

func connect_engine() -> void:
	received.clear()
	outgoing.clear()
	tcp = StreamPeerTCP.new()
	tcp.connect_to_host("127.0.0.1", port)
	retry_at = Time.get_ticks_msec() + 2000

func disconnect_engine(message: String) -> void:
	script_editor.disconnected()
	avatar_restore_started = false
	reset_media_clock()
	asset_importer.disconnected()
	cancel_hand_grab()
	set_remote_previews([])
	if gizmo.dragging: gizmo.cancel_drag()
	if gizmo_preview_active: restore_gizmo_visual()
	tcp.disconnect_from_host()
	connected = false
	editable = false
	pending = false
	pending_edit.clear()
	inspector_edits.clear()
	dirty.clear()
	status_label.text = message
	retry_at = Time.get_ticks_msec() + 2000
	jump_requested = false

func send_player_input() -> void:
	if not connected: return
	var movement := Vector2.ZERO
	var jump_held: bool = Input.is_action_pressed("jump") and get_viewport().gui_get_focus_owner() == null
	var active: bool = player_initialized and avatar_camera and not gizmo.dragging and (fps_active or look or jump_requested or jump_held)
	if active and (look or fps_active):
		movement = Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	queue_message({"kind":"player_input", "input":{
		"movement":[movement.x, movement.y], "yaw":orbit_yaw, "pitch":-orbit_pitch,
		"jump":active and (jump_requested or jump_held),
		"face_view":first_person, "active":active}})
	jump_requested = false

func release_player_input() -> void:
	jump_requested = false
	if connected: queue_message({"kind":"player_input", "input":{
		"movement":[0, 0], "yaw":0, "pitch":0, "jump":false, "face_view":false, "active":false}})

func begin_hand_grab() -> void:
	if not editable or pending or grab_waiting or not hand_grab.is_empty(): return
	grab_waiting = true
	send_player_input()
	queue_message({"kind":"grab","action":"begin"})

func finish_hand_grab() -> void:
	if hand_grab.is_empty() or grab_releasing: return
	grab_releasing = true
	pending = true
	pending_action = "grab"
	queue_message({"kind":"grab","action":"release"})

func cancel_hand_grab() -> void:
	if connected and (grab_waiting or not hand_grab.is_empty()) and not grab_releasing:
		queue_message({"kind":"grab","action":"cancel"})
	grab_waiting = false
	hand_grab.clear()
	right_down = false

func release_cursor() -> void:
	cancel_hand_grab()
	fps_active = false
	look = false
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	release_player_input()

func draw_interaction_rays() -> void:
	laser_mesh.clear_surfaces()
	var points: Array[Vector3] = []
	var colors: Array[Color] = []
	for entry in interaction_rays:
		var ray: Dictionary = entry.ray
		var local: bool = entry.owner == "local"
		if local and (not fps_active or ray.target == null): continue
		if not local and not ray.holding: continue
		var o: Array = ray.origin
		var p: Array = ray.point
		var start := Vector3(o[0],o[1],o[2])
		var end := Vector3(p[0],p[1],p[2])
		if ray.holding and ray.target is Dictionary:
			var target: String = ray.target.instance_id
			if remote_previews.has(target) and render_nodes.has(target):
				var preview: Dictionary = remote_previews[target]
				var pos: Array = preview.position
				var rot: Array = preview.get("rotation", objects[target].world_rotation)
				var size: Array = preview.get("scale", objects[target].world_scale)
				var frame := Transform3D(Basis(Quaternion(rot[0],rot[1],rot[2],rot[3])) * Basis.from_scale(Vector3(size[0],size[1],size[2])), Vector3(pos[0],pos[1],pos[2]))
				end = render_nodes[target].transform * (frame.affine_inverse() * end)
		if local: start = camera.position + camera.global_basis.x * 0.12 - camera.global_basis.y * 0.09
		var tint := Color("ffd087") if ray.holding else Color("a2ece7")
		points.append_array([start,end])
		colors.append_array([tint,tint])
		for axis in [Vector3.RIGHT,Vector3.UP,Vector3.BACK]:
			points.append_array([end-axis*0.025,end+axis*0.025])
			colors.append_array([tint,tint])
	if points.is_empty(): return
	laser_mesh.surface_begin(Mesh.PRIMITIVE_LINES)
	for i in range(points.size()):
		laser_mesh.surface_set_color(colors[i])
		laser_mesh.surface_add_vertex(points[i])
	laser_mesh.surface_end()

func _notification(what: int) -> void:
	if what == NOTIFICATION_APPLICATION_FOCUS_OUT:
		release_cursor()

func reset_media_clock() -> void:
	clock_ready = false
	clock_best_rtt = INF
	clock_next_ping = 0.0
	clock_requests.clear()

func update_media_clock() -> void:
	var now := Time.get_ticks_msec() / 1000.0
	if not connected or not editable or now < clock_next_ping: return
	clock_next_ping = now + 2.0
	for key in clock_requests.keys():
		if now - float(clock_requests[key]) > 10.0: clock_requests.erase(key)
	var request := str(Time.get_ticks_usec())
	clock_requests[request] = now
	queue_message({"kind":"clock_ping","request_id":request,"client_time":now})

func accept_clock(body: Dictionary) -> void:
	var request := str(body.get("request_id",""))
	if not clock_requests.has(request): return
	var sent: float = clock_requests[request]
	clock_requests.erase(request)
	var now := Time.get_ticks_msec() / 1000.0
	var rtt := now - sent
	var server := float(body.get("server_time",-1.0))
	if not is_finite(server) or server < 0.0 or rtt < 0.0 or rtt > 5.0: return
	if not clock_ready or rtt <= clock_best_rtt * 1.5 + 0.01:
		clock_offset = server - (sent + now) * 0.5
		clock_best_rtt = minf(clock_best_rtt,rtt)
		clock_ready = true

func media_time() -> float:
	return Time.get_ticks_msec() / 1000.0 + clock_offset

func send_playback(action: String, values: Dictionary = {}) -> void:
	if not editable or pending or not hand_grab.is_empty() or current.is_empty() or current.get("playback") == null: return
	var command := {"command_id":str(Time.get_ticks_usec()),"target":current.address,"expected_revision":fields.playback.revision,"action":action}
	command.merge(values)
	pending = true
	pending_action = "playback"
	queue_message({"kind":"playback","command":command})

func update_video_controls() -> void:
	if not video_controls.visible or current.get("playback") == null: return
	var duration: float = asset_importer.video_duration(current)
	var position: float = asset_importer.video_position(current,duration) if clock_ready else 0.0
	video_time.text = "Loading video…" if duration <= 0.0 else (("%.1f / %.1f s" % [position,duration]) if clock_ready else "Waiting for host clock…")
	video_seek.editable = editable and clock_ready and not pending and duration > 0.0
	if not seeking_video:
		video_seek.max_value = maxf(0.1,duration)
		video_seek.set_value_no_signal(position)
	filling = true
	video_loop.button_pressed = current.playback.looping
	video_loop.disabled = not editable or pending
	filling = false

func _process(delta: float) -> void:
	script_editor.tick()
	tcp.poll()
	var state := tcp.get_status()
	if state == StreamPeerTCP.STATUS_CONNECTED:
		if not connected:
			connected = true
			tcp.set_no_delay(true)
			queue_message({"kind":"hello"})
		if not outgoing.is_empty():
			var sent := tcp.put_partial_data(outgoing.slice(0, 524288))
			if sent[0] != OK: disconnect_engine("Connection lost")
			else: outgoing = outgoing.slice(sent[1])
		var available := mini(tcp.get_available_bytes(), 524288)
		if available > 0:
			var part := tcp.get_partial_data(available)
			if part[0] == OK: received.append_array(part[1])
			else: disconnect_engine("Connection lost")
		for _i in range(8):
			if received.size() < 4: break
			var size := (received[0] << 24) | (received[1] << 16) | (received[2] << 8) | received[3]
			if size <= 0 or size > MAX_FRAME:
				disconnect_engine("Invalid engine frame")
				break
			if received.size() < size + 4: break
			var decoded: Variant = JSON.parse_string(received.slice(4, size + 4).get_string_from_utf8())
			received = received.slice(size + 4)
			if decoded is Dictionary: accept_message(decoded)
	else:
		if connected: disconnect_engine("Engine disconnected. Reconnecting…")
		if Time.get_ticks_msec() >= retry_at: connect_engine()
	flush_inspector_edits()
	if editable and not avatar_restore_started:
		avatar_restore_started = true
		if FileAccess.file_exists(asset_importer.avatar_file()): asset_importer.queue_avatar_texture(asset_importer.avatar_file())
	update_media_clock()
	var busy := pending or grab_waiting or not hand_grab.is_empty()
	avatar_button.disabled = not editable or busy
	reset_avatar.disabled = not editable or busy
	for button in apply_buttons: button.disabled = not editable or busy or current.is_empty()
	create_button.disabled = not editable or busy or objects.size() >= 128
	image_button.disabled = create_button.disabled
	duplicate_button.disabled = not editable or busy or current.is_empty() or objects.size() >= 128
	delete_button.disabled = not editable or busy or current.is_empty()
	crosshair.visible = first_person and fps_active
	crosshair.modulate = Color("ffd087") if not hand_grab.is_empty() else (Color("a2ece7") if aiming_at_collider else Color.WHITE)
	if connected and gizmo_preview_active and Time.get_ticks_msec() - last_preview_at >= 50:
		last_preview_at = Time.get_ticks_msec()
		send_gizmo_preview()
	for id in remote_previews.keys():
		var preview: Dictionary = remote_previews[id]
		if not valid_remote_preview(preview):
			remote_previews.erase(id)
			continue
		if gizmo_preview_active and gizmo_target == id: continue
		var p: Array = preview.position
		var node: MeshInstance3D = render_nodes[id]
		var alpha := 1.0 - exp(-20.0 * delta)
		node.position = node.position.lerp(Vector3(p[0], p[1], p[2]), alpha)
		var q: Array = preview.rotation if preview.get("rotation") != null else objects[id].world_rotation
		var s: Array = preview.scale if preview.get("scale") != null else objects[id].world_scale
		node.quaternion = node.quaternion.slerp(Quaternion(q[0],q[1],q[2],q[3]),alpha)
		node.scale = node.scale.lerp(Vector3(s[0],s[1],s[2]),alpha)
	update_selection_outline()
	if not current.is_empty(): gizmo.set_target(cube.position, editable and not busy and not look and not fps_active)
	else: gizmo.clear_target()
	if look and not gizmo.dragging and not avatar_camera:
		var movement := Vector3.ZERO
		if Input.is_physical_key_pressed(KEY_W): movement.z -= 1
		if Input.is_physical_key_pressed(KEY_S): movement.z += 1
		if Input.is_physical_key_pressed(KEY_A): movement.x -= 1
		if Input.is_physical_key_pressed(KEY_D): movement.x += 1
		if Input.is_physical_key_pressed(KEY_Q): movement.y -= 1
		if Input.is_physical_key_pressed(KEY_E): movement.y += 1
		camera.position += camera.global_basis * movement.normalized() * delta * 5
	if player_initialized:
		player_position = player_position.lerp(player_target_position, 1.0 - exp(-35.0 * delta))
	if avatar_camera and player_initialized and not gizmo.dragging:
		if first_person:
			camera.position = player_position + Vector3(0, 1.15, 0)
			camera.rotation = Vector3(orbit_pitch, orbit_yaw, 0)
		else:
			var target := player_position + Vector3(0, 1.0, 0)
			camera.position = target + Vector3(sin(orbit_yaw) * cos(orbit_pitch), -sin(orbit_pitch), cos(orbit_yaw) * cos(orbit_pitch)) * orbit_distance
			camera.look_at(target)
	if avatars.has(self_id):
		avatars[self_id].visible = not first_person or not avatar_camera
		avatars[self_id].target_position = player_position
		avatars[self_id].target_yaw = player_yaw
	if connected and Time.get_ticks_msec() - last_input_at >= 16:
		last_input_at = Time.get_ticks_msec()
		send_player_input()
	draw_interaction_rays()
	asset_importer.update_videos()
	update_video_controls()
	frames += 1
	if not capture_path.is_empty() and frames == 90:
		await RenderingServer.frame_post_draw
		get_viewport().get_texture().get_image().save_png(capture_path)
		get_tree().quit()

func accept_message(message: Dictionary) -> void:
	if message.get("protocol_version") != 2 or message.get("session_id") != session: return
	if epoch != message.authority_epoch:
		reset_media_clock()
		avatar_restore_started = false
	epoch = message.authority_epoch
	var body: Dictionary = message.body
	if str(body.get("kind","")).begins_with("asset_"):
		asset_importer.receive(body)
		return
	match body.get("kind"):
		"clock_pong":
			accept_clock(body)
		"script_statuses":
			script_statuses = body.statuses
			script_editor.refresh()
		"interaction_rays":
			interaction_rays = body.rays
			aiming_at_collider = false
			for entry in interaction_rays:
				if entry.owner == "local": aiming_at_collider = entry.ray.target != null
		"grab_state":
			grab_waiting = false
			hand_grab = body.grab if body.get("grab") is Dictionary else {}
			if hand_grab.is_empty():
				grab_releasing = false
			else:
				if not pending: select_object(hand_grab.preview.target.instance_id)
				feedback.text = "Holding · E: rotate · Shift+Wheel: scale · LMB: straighten · Release RMB: place"
				if not right_down and not hand_grab.released: finish_hand_grab()
		"grab_error":
			grab_waiting = false
			feedback.text = body.message
		"object_previews":
			set_remote_previews(body.previews)
		"presence":
			self_id = body.self_id
			if body.get("local_motion") is Dictionary: grounded = body.local_motion.grounded
			var visible := {}
			var names: PackedStringArray = []
			for person in body.participants:
				var id: String = person.id
				visible[id] = true
				var fresh := not avatars.has(id)
				if fresh:
					var avatar = AVATAR.new()
					add_child(avatar)
					avatars[id] = avatar
				avatars[id].set_state(person, id == self_id, fresh)
				var texture_id: String = person.avatar_texture if person.get("avatar_texture") is String else ""
				asset_importer.attach_avatar(id,texture_id)
				if id == self_id:
					var p: Array = person.pose.position
					player_target_position = Vector3(p[0], p[1], p[2])
					player_yaw = person.pose.yaw
					if not player_initialized:
						player_position = player_target_position
						player_initialized = true
					if not nickname.has_focus(): nickname.text = person.name
				names.append(person.name + (" (you)" if id == self_id else ""))
			for id in avatars.keys():
				if not visible.has(id):
					avatars[id].queue_free()
					avatars.erase(id)
			people_label.text = "\n".join(names)
		"snapshot":
			editable = body.editable
			script_templates = body.get("script_templates", [])
			script_statuses = body.get("script_statuses", [])
			DisplayServer.window_set_title("Kokoverse Workshop — " + str(body.role).capitalize())
			status_label.text = ("Host" if body.role == "host" else "Guest") + (" · Connected" if editable else " · Waiting for host")
			apply_snapshot(body.snapshot)
			script_editor.refresh()
		"status":
			status_label.text = body.message
			editable = body.editable
			if not editable:
				script_editor.disconnected()
				avatar_restore_started = false
				reset_media_clock()
				set_remote_previews([])
				if gizmo.dragging: gizmo.cancel_drag()
				if gizmo_preview_active: restore_gizmo_visual()
				if pending: feedback.text = "Connection interrupted. Check the shared value before retrying."
				pending = false
				pending_edit.clear()
				inspector_edits.clear()
				dirty.clear()
		"result":
			if script_editor.receive_result(body):
				if body.has("snapshot"): apply_snapshot(body.snapshot)
				return
			if pending_action == "gizmo": restore_gizmo_visual()
			pending = false
			if pending_action == "edit" and not pending_edit.is_empty():
				var key := inspector_key(pending_edit.target,pending_edit.field_id)
				if not body.ok: inspector_edits.erase(key)
				if not current.is_empty() and current.address == pending_edit.target and not inspector_edits.has(key):
					dirty.erase(pending_edit.field_id)
			else: dirty.clear()
			feedback.text = "Applied to the shared world." if body.ok else body.get("message", "Edit rejected")
			if body.has("snapshot"): apply_snapshot(body.snapshot)
			else: refresh_fields()
			if pending_action == "edit" and body.ok and not pending_edit.is_empty():
				var key := inspector_key(pending_edit.target,pending_edit.field_id)
				if inspector_edits.has(key) and objects.has(pending_edit.target.instance_id):
					for field in objects[pending_edit.target.instance_id].fields:
						if field.id == pending_edit.field_id:
							inspector_edits[key].expected_revision = field.revision
							if not current.is_empty() and current.address == pending_edit.target: dirty[field.id] = field.revision
			pending_edit.clear()
			if body.ok and pending_action in ["create", "create_image", "duplicate"] and body.get("selected") is Dictionary:
				select_object(body.selected.instance_id)
				focus_selected()
			pending_action = ""
			asset_importer.on_result(body)

func apply_snapshot(snapshot: Dictionary) -> void:
	if gizmo.dragging:
		var valid_target := false
		for object in snapshot.objects:
			if object.address.instance_id == gizmo_target:
				for field in object.fields:
					if field.id == "position" and field.revision == gizmo_revision: valid_target = true
		if not valid_target:
			gizmo.cancel_drag()
			feedback.text = "Object changed elsewhere. Move cancelled."
	objects.clear()
	var used_images := {}
	for object in snapshot.objects:
		var id: String = object.address.instance_id
		objects[id] = object
		if not render_nodes.has(id):
			var node := MeshInstance3D.new()
			var image_panel: bool = object.get("visual", {}).get("kind", "cube") in ["image","video"]
			if image_panel:
				var quad := QuadMesh.new()
				var ratio: float = float(object.visual.width) / float(object.visual.height)
				quad.size = Vector2(2.0, 2.0 / ratio) if ratio >= 1 else Vector2(2.0 * ratio, 2.0)
				node.mesh = quad
			else:
				var mesh := BoxMesh.new()
				mesh.size = Vector3(1.5, 1.5, 1.5)
				node.mesh = mesh
			var mat := StandardMaterial3D.new()
			mat.roughness = 0.45
			if image_panel:
				mat.cull_mode = BaseMaterial3D.CULL_DISABLED
				mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA_SCISSOR
				mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
			node.material_override = mat
			var body := StaticBody3D.new()
			body.set_meta("object_id", id)
			var collision := CollisionShape3D.new()
			collision.name = "Collider"
			var shape := BoxShape3D.new()
			shape.size = Vector3.ONE
			collision.shape = shape
			body.add_child(collision)
			node.add_child(body)
			body.name = "PickBody"
			add_child(node)
			render_nodes[id] = node
		var node: MeshInstance3D = render_nodes[id]
		var collision: CollisionShape3D = node.get_node("PickBody/Collider")
		collision.disabled = object.get("collider") == null
		if object.get("collider") is Dictionary:
			var half: Array = object.collider.half_extents
			collision.shape.size = Vector3(half[0],half[1],half[2])*2.0
		if object.get("visual", {}).get("kind", "cube") == "image":
			var source: String = object.visual.data_url
			used_images[source] = true
			if not image_textures.has(source):
				var image := Image.new()
				var bytes := Marshalls.base64_to_raw(source.get_slice(",", 1))
				var error := image.load_png_from_buffer(bytes) if source.begins_with("data:image/png") else image.load_jpg_from_buffer(bytes)
				if error == OK: image_textures[source] = ImageTexture.create_from_image(image)
			node.material_override.albedo_texture = image_textures.get(source)
		apply_object_transform(node,object)
		if gizmo_preview_active and id == gizmo_target: node.position = gizmo_preview
		for field in object.fields:
			if field.id == "color":
				var c: Array = field.value
				node.material_override.albedo_color = Color(c[0], c[1], c[2], c[3])
		if object.visual.kind == "model":
			asset_importer.attach_model(id)
			if node.has_node("ImportedModel"): asset_importer.tint_model(node.get_node("ImportedModel"),node.material_override.albedo_color)
		if object.visual.kind == "video": asset_importer.attach_video(id)
	for id in render_nodes.keys():
		if not objects.has(id):
			render_nodes[id].queue_free()
			render_nodes.erase(id)
	for source in image_textures.keys():
		if not used_images.has(source): image_textures.erase(source)
	if not objects.has(selected_id):
		selected_id = "" if objects.is_empty() else str(objects.keys()[0])
		dirty.clear()
	revision_label.text = "World revision " + snapshot.world_revision + " · ObjectRoot"
	update_selected()
	rebuild_tree()

func select_object(id: String) -> void:
	if pending: return
	if not objects.has(id): return
	if id != selected_id and gizmo.dragging: gizmo.cancel_drag()
	if id != selected_id: dirty.clear()
	selected_id = id
	update_selected()
	rebuild_tree()

func update_selected() -> void:
	current = objects.get(selected_id, {})
	video_controls.visible = not current.is_empty() and current.visual.kind == "video"
	fields.clear()
	selection_outline.visible = not current.is_empty()
	if current.is_empty():
		object_label.text = "No object selected"
		name_input.text = ""
		return
	for field in current.fields: fields[field.id] = field
	cube = render_nodes[selected_id]
	material = cube.material_override
	update_selection_outline()
	object_label.text = fields.name.value
	refresh_fields()

func rebuild_tree() -> void:
	var entries: Array = []
	for id in objects:
		var name := "Cube"
		for field in objects[id].fields:
			if field.id == "name": name = field.value
		entries.append([id, name])
	var signature := JSON.stringify([selected_id, entries])
	if signature == world_tree_signature: return
	world_tree_signature = signature
	for node in tree_objects.get_children():
		tree_objects.remove_child(node)
		node.queue_free()
	for id in objects:
		var object: Dictionary = objects[id]
		var button := Button.new()
		var name := "Cube"
		for field in object.fields:
			if field.id == "name": name = field.value
		button.text = "◇  " + name
		button.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.custom_minimum_size = Vector2(180, 36)
		button.toggle_mode = true
		button.button_pressed = id == selected_id
		button.pressed.connect(select_object.bind(id))
		tree_objects.add_child(button)
	object_count.text = "%d / 128 objects" % objects.size()

func focus_selected() -> void:
	if current.is_empty(): return
	release_cursor()
	avatar_camera = false
	release_player_input()
	first_person = false
	view_mode.select(2)
	camera.position = cube.position + Vector3(5, 3.25, 8)
	camera.look_at(cube.position)

func set_view_mode(mode: int) -> void:
	if gizmo.dragging: gizmo.cancel_drag()
	cancel_hand_grab()
	look = false
	avatar_camera = mode != 2
	release_player_input()
	first_person = mode == 1
	fps_active = first_person
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if fps_active else Input.MOUSE_MODE_VISIBLE
	view_mode.select(mode)
	view_mode.release_focus()

func try_jump() -> void:
	if player_initialized and avatar_camera and not gizmo.dragging:
		jump_requested = true
		send_player_input()

func refresh_fields() -> void:
	if fields.is_empty(): return
	filling = true
	if not dirty.has("name"): name_input.text = fields.name.value
	if not dirty.has("position"):
		for i in range(3): position_inputs[i].value = fields.position.value[i]
	if not dirty.has("color"):
		var c: Array = fields.color.value
		color_input.color = Color(c[0], c[1], c[2], c[3])
	if fields.has("rotation") and not dirty.has("rotation"):
		var q: Array = fields.rotation.value
		var angles := Quaternion(q[0],q[1],q[2],q[3]).get_euler()
		for i in range(3): rotation_inputs[i].value = rad_to_deg(angles[i])
	if fields.has("scale") and not dirty.has("scale"):
		for i in range(3): scale_inputs[i].value = fields.scale.value[i]
	if fields.has("collider") and not dirty.has("collider"):
		var shape: Variant = fields.collider.value
		collider_enabled.button_pressed = shape != null
		collider_solid.button_pressed = shape != null and shape.solid
		for i in range(3): collider_inputs[i].value = shape.half_extents[i] if shape != null else 0.5
	filling = false

func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo and event.physical_keycode == KEY_I:
		var focus := get_viewport().gui_get_focus_owner()
		if fps_active or not (focus is LineEdit or focus is TextEdit):
			inspector_toggle.button_pressed = not inspector_toggle.button_pressed
			get_viewport().set_input_as_handled()
			return
	# Captured FPS mouse input belongs to the viewport, even over an inspector panel.
	if fps_active and (event is InputEventMouse or (event is InputEventKey and event.keycode == KEY_ESCAPE)):
		_unhandled_input(event)
		get_viewport().set_input_as_handled()

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.keycode == KEY_ESCAPE and event.pressed:
		release_cursor()
		get_viewport().set_input_as_handled()
		return
	if event is InputEventMouseButton and first_person and avatar_camera:
		if event.button_index == MOUSE_BUTTON_RIGHT:
			right_down = event.pressed
			if event.pressed:
				fps_active = true
				Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
				begin_hand_grab()
			else: finish_hand_grab()
			get_viewport().set_input_as_handled()
			return
		if event.pressed and event.button_index in [MOUSE_BUTTON_WHEEL_UP,MOUSE_BUTTON_WHEEL_DOWN] and not hand_grab.is_empty():
			if event.shift_pressed:
				queue_message({"kind":"grab","action":"scale","factor":1.1 if event.button_index == MOUSE_BUTTON_WHEEL_UP else 1.0/1.1})
			else:
				queue_message({"kind":"grab","action":"distance","delta":0.25 if event.button_index == MOUSE_BUTTON_WHEEL_UP else -0.25})
			get_viewport().set_input_as_handled()
			return
		if event.pressed and event.button_index in [MOUSE_BUTTON_WHEEL_LEFT,MOUSE_BUTTON_WHEEL_RIGHT] and not hand_grab.is_empty():
			queue_message({"kind":"grab","action":"rotate","delta":[0.15 if event.button_index == MOUSE_BUTTON_WHEEL_RIGHT else -0.15,0.0],"vertical":true})
			get_viewport().set_input_as_handled()
			return
		if event.pressed and event.button_index == MOUSE_BUTTON_LEFT and not hand_grab.is_empty():
			queue_message({"kind":"grab","action":"align"})
			get_viewport().set_input_as_handled()
			return
		if event.pressed and event.button_index == MOUSE_BUTTON_LEFT and not fps_active:
			fps_active = true
			Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
			get_viewport().set_input_as_handled()
			return
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		if not fps_active and gizmo.begin_drag(event.position):
			get_viewport().set_input_as_handled()
			return
		var origin := camera.project_ray_origin(event.position)
		var query := PhysicsRayQueryParameters3D.create(origin, origin + camera.project_ray_normal(event.position) * 5000)
		var hit := get_world_3d().direct_space_state.intersect_ray(query)
		if not hit.is_empty() and hit.collider.has_meta("object_id"):
			var hit_id := str(hit.collider.get_meta("object_id"))
			select_object(hit_id)
			script_editor.interact(hit_id)
	if event is InputEventKey and event.keycode == KEY_F and event.pressed: focus_selected()
	if event is InputEventKey and event.keycode == KEY_V and event.pressed: set_view_mode((view_mode.selected + 1) % 3)
	if event is InputEventKey and event.keycode == KEY_SPACE and event.pressed and not event.echo: try_jump()
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_RIGHT:
		look = event.pressed
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if look else Input.MOUSE_MODE_VISIBLE
	if event is InputEventMouseMotion and (look or fps_active):
		if not hand_grab.is_empty() and Input.is_action_pressed("grab_rotate"):
			queue_message({"kind":"grab","action":"rotate","delta":[clampf(event.relative.x*0.006,-1.0,1.0),clampf(event.relative.y*0.006,-1.0,1.0)],"vertical":event.shift_pressed})
			return
		if avatar_camera:
			orbit_yaw -= event.relative.x * 0.003
			orbit_pitch = clampf(orbit_pitch - event.relative.y * 0.003, -1.4 if first_person else -1.2, 1.4 if first_person else 0.2)
		else:
			camera.rotation.y -= event.relative.x * 0.003
			camera.rotation.x = clampf(camera.rotation.x - event.relative.y * 0.003, -1.4, 1.4)
	if event is InputEventKey and event.keycode == KEY_ESCAPE:
		look = false
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		release_player_input()
