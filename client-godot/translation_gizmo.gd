extends Node3D
## Runtime world-axis translation handles. Preview is local; the owner commits on release.
signal drag_started
signal preview_changed(world_position: Vector3)
signal drag_finished(world_position: Vector3)
signal drag_cancelled

const AXES: Array[Vector3] = [Vector3.RIGHT, Vector3.UP, Vector3.BACK]
const COLORS: Array[Color] = [Color("f47979"), Color("7be19c"), Color("76a9ff")]
var camera: Camera3D
var enabled := false
var dragging := false
var active_axis := -1
var hovered_axis := -1
var drag_origin := Vector3.ZERO
var plane_normal := Vector3.ZERO
var start_offset := 0.0
var unit_scale := 1.0
var materials: Array[StandardMaterial3D] = []

func _ready() -> void:
	visible = false
	for i in range(3):
		var material := StandardMaterial3D.new()
		material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		material.no_depth_test = true
		material.render_priority = 120
		material.albedo_color = COLORS[i]
		materials.append(material)
		var shaft := MeshInstance3D.new()
		var cylinder := CylinderMesh.new()
		cylinder.top_radius = 0.025
		cylinder.bottom_radius = 0.025
		cylinder.height = 1.05
		shaft.mesh = cylinder
		shaft.material_override = material
		shaft.position = AXES[i] * 0.725
		shaft.quaternion = Quaternion(Vector3.UP, AXES[i])
		add_child(shaft)
		var tip := MeshInstance3D.new()
		var cone := CylinderMesh.new()
		cone.top_radius = 0
		cone.bottom_radius = 0.09
		cone.height = 0.30
		tip.mesh = cone
		tip.material_override = material
		tip.position = AXES[i] * 1.4
		tip.quaternion = Quaternion(Vector3.UP, AXES[i])
		add_child(tip)
		var label := Label3D.new()
		label.text = ["X", "Y", "Z"][i]
		label.position = AXES[i] * 1.72
		label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
		label.no_depth_test = true
		label.modulate = COLORS[i]
		label.font_size = 48
		label.pixel_size = 0.003
		add_child(label)

func set_target(world_position: Vector3, interactive: bool) -> void:
	visible = true
	enabled = interactive
	if not dragging: global_position = world_position

func clear_target() -> void:
	if dragging: cancel_drag()
	visible = false
	enabled = false

func _process(_delta: float) -> void:
	if not visible or camera == null: return
	if camera.is_position_behind(global_position):
		for child in get_children(): child.visible = false
		return
	for child in get_children(): child.visible = true
	if not dragging:
		var screen := camera.unproject_position(global_position)
		var depth := -camera.to_local(global_position).z
		unit_scale = clampf(camera.project_position(screen + Vector2(80, 0), depth).distance_to(global_position), 0.03, 100.0)
		scale = Vector3.ONE * unit_scale
		hovered_axis = pick_axis(get_viewport().get_mouse_position()) if enabled and get_viewport().gui_get_hovered_control() == null else -1
	for i in range(3):
		materials[i].albedo_color = Color("ffe69a") if i == active_axis or i == hovered_axis else COLORS[i]

func pick_axis(screen: Vector2) -> int:
	if not visible or not enabled or camera == null or camera.is_position_behind(global_position): return -1
	var best := -1
	var distance := 12.0
	for i in range(3):
		var a := camera.unproject_position(global_position + AXES[i] * unit_scale * 0.24)
		var b := camera.unproject_position(global_position + AXES[i] * unit_scale * 1.55)
		var segment := b - a
		if segment.length_squared() < 64: continue
		var t := clampf((screen - a).dot(segment) / segment.length_squared(), 0, 1)
		var d := screen.distance_to(a + t * segment)
		if d < distance:
			distance = d
			best = i
	return best

func begin_drag(screen: Vector2) -> bool:
	var axis := pick_axis(screen)
	if axis < 0: return false
	var view_direction := camera.global_position - global_position
	plane_normal = view_direction - AXES[axis] * view_direction.dot(AXES[axis])
	if plane_normal.length_squared() < 0.0001: return false
	plane_normal = plane_normal.normalized()
	drag_origin = global_position
	active_axis = axis
	var point: Variant = point_on_drag_plane(screen)
	if point == null:
		active_axis = -1
		return false
	start_offset = (point - drag_origin).dot(AXES[axis])
	dragging = true
	drag_started.emit()
	return true

func point_on_drag_plane(screen: Vector2) -> Variant:
	var origin := camera.project_ray_origin(screen)
	var direction := camera.project_ray_normal(screen)
	var denominator := direction.dot(plane_normal)
	if absf(denominator) < 0.00001: return null
	var distance := (drag_origin - origin).dot(plane_normal) / denominator
	if distance < 0: return null
	return origin + direction * distance

func update_drag(screen: Vector2, snap: bool = false) -> void:
	if not dragging: return
	var point: Variant = point_on_drag_plane(screen)
	if point == null: return
	var amount: float = (point - drag_origin).dot(AXES[active_axis]) - start_offset
	if snap: amount = snappedf(amount, 0.25)
	var position := drag_origin + AXES[active_axis] * amount
	position[active_axis] = clampf(position[active_axis], -1000.0, 1000.0)
	global_position = position
	preview_changed.emit(position)

func finish_drag() -> void:
	if not dragging: return
	dragging = false
	active_axis = -1
	drag_finished.emit(global_position)

func cancel_drag() -> void:
	if not dragging: return
	dragging = false
	active_axis = -1
	global_position = drag_origin
	drag_cancelled.emit()

func _input(event: InputEvent) -> void:
	if not dragging: return
	if event is InputEventMouseMotion:
		update_drag(event.position, event.shift_pressed)
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and not event.pressed:
		update_drag(event.position, event.shift_pressed)
		finish_drag()
		get_viewport().set_input_as_handled()
	elif event is InputEventKey and event.keycode == KEY_ESCAPE and event.pressed:
		cancel_drag()
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_RIGHT and event.pressed:
		cancel_drag()
		get_viewport().set_input_as_handled()

func _notification(what: int) -> void:
	if what == NOTIFICATION_WM_WINDOW_FOCUS_OUT and dragging: cancel_drag()
