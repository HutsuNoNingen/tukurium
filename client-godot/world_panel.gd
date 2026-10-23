extends MeshInstance3D
## A shared surface. Its content is a separate inspector view, not the world model.
const INSPECTOR_VIEW = preload("res://inspector_view.gd")
var panel_id := ""
var viewport := SubViewport.new()
var content = INSPECTOR_VIEW.new()
var manager
var last_pointer := Vector2(-1000, -1000)

func setup(controller, id: String) -> void:
	manager = controller
	panel_id = id
	var quad := QuadMesh.new()
	quad.size = Vector2(1.2, 1.6)
	mesh = quad
	viewport.name = "PanelViewport"
	viewport.size = Vector2i(720, 960)
	viewport.transparent_bg = false
	viewport.disable_3d = true
	viewport.gui_embed_subwindows = true
	viewport.render_target_update_mode = SubViewport.UPDATE_WHEN_VISIBLE
	add_child(viewport)
	viewport.add_child(content)
	content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	content.setup(controller, id)
	var surface := StandardMaterial3D.new()
	surface.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	surface.albedo_texture = viewport.get_texture()
	surface.cull_mode = BaseMaterial3D.CULL_DISABLED
	surface.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR
	material_override = surface
	var body := StaticBody3D.new()
	body.name = "PickBody"
	body.set_meta("object_id", id)
	var collision := CollisionShape3D.new()
	collision.name = "Collider"
	var shape := BoxShape3D.new()
	shape.size = Vector3(1.2, 1.6, 0.05)
	collision.shape = shape
	body.add_child(collision)
	add_child(body)

func refresh(object: Dictionary) -> void:
	if not manager.windows.has(panel_id):
		viewport.size = Vector2i(int(object.panel.size[0]), int(object.panel.size[1]))
	content.refresh(object)

func canvas_point(global_point: Vector3) -> Vector2:
	var local := to_local(global_point)
	return Vector2((local.x / 1.2 + 0.5) * viewport.size.x, (0.5 - local.y / 1.6) * viewport.size.y)

func forward_pointer(event: InputEventMouse, point: Vector2) -> void:
	var copy: InputEventMouse = event.duplicate()
	copy.position = point
	copy.global_position = point
	if copy is InputEventMouseMotion:
		copy.relative = point - last_pointer
	last_pointer = point
	viewport.push_input(copy, true)

func leave_pointer() -> void:
	var event := InputEventMouseMotion.new()
	forward_pointer(event, Vector2(-1000, -1000))

func clear_focus() -> void:
	viewport.gui_release_focus()
	content.finish_all()

func has_text_focus() -> bool:
	var focus := viewport.gui_get_focus_owner()
	return focus is LineEdit or focus is TextEdit
