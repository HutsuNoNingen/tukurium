extends Node
## Local planar reflections for shared Mirror components. Viewports stay local.
const MIRROR_LAYER := 1 << 19
var app
var entries := {}

func setup(owner) -> void:
	app = owner

func is_mirror(object: Dictionary) -> bool:
	return object.get("mirror") == true and object.get("visual", {}).get("kind", "") == "primitive" and object.visual.get("shape", "") in ["quad", "mirror"]

func set_surface(node: MeshInstance3D, reflective: bool, shape: String) -> void:
	if not is_instance_valid(node): return
	var material: StandardMaterial3D = node.material_override
	material.albedo_texture = null
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED if reflective else BaseMaterial3D.SHADING_MODE_PER_PIXEL
	material.cull_mode = BaseMaterial3D.CULL_BACK if reflective else (BaseMaterial3D.CULL_DISABLED if shape == "quad" else BaseMaterial3D.CULL_BACK)
	material.uv1_scale = Vector3(-1,1,1) if reflective else Vector3.ONE
	material.uv1_offset = Vector3(1,0,0) if reflective else Vector3.ZERO
	node.layers = MIRROR_LAYER if reflective else 1

func sync(objects: Dictionary, nodes: Dictionary) -> void:
	for id in entries.keys():
		if not objects.has(id) or not nodes.has(id) or not is_mirror(objects[id]) or entries[id].node != nodes[id]:
			if objects.has(id): set_surface(entries[id].node, false, str(objects[id].get("visual",{}).get("shape","")))
			entries[id].viewport.queue_free()
			entries.erase(id)
	for id in objects:
		if not is_mirror(objects[id]) or not nodes.has(id) or entries.has(id): continue
		var node: MeshInstance3D = nodes[id]
		set_surface(node, true, str(objects[id].visual.shape))
		var viewport := SubViewport.new()
		viewport.size = Vector2i(512, 512)
		viewport.world_3d = app.get_world_3d()
		viewport.render_target_update_mode = SubViewport.UPDATE_DISABLED
		add_child(viewport)
		var reflection := Camera3D.new()
		reflection.projection = Camera3D.PROJECTION_FRUSTUM
		reflection.cull_mask = 0xfffff ^ MIRROR_LAYER
		reflection.current = true
		viewport.add_child(reflection)
		node.material_override.albedo_texture = viewport.get_texture()
		entries[id] = {"node":node, "viewport":viewport, "camera":reflection}

func tick() -> void:
	if entries.is_empty(): return
	var viewer: Camera3D = app.camera_tool.eye if app.camera_tool.active else app.camera
	for id in entries:
		var node: MeshInstance3D = entries[id].node
		if not is_instance_valid(node) or not node.is_visible_in_tree(): continue
		var normal: Vector3 = node.global_basis.z.normalized()
		var distance: float = normal.dot(viewer.global_position - node.global_position)
		if distance > 0.05 and distance < 4000.0 and not viewer.is_position_behind(node.global_position):
			update_reflection(entries[id], viewer)

func update_reflection(entry: Dictionary, viewer: Camera3D) -> void:
	var node: MeshInstance3D = entry.node
	var camera: Camera3D = entry.camera
	var viewport: SubViewport = entry.viewport
	var normal: Vector3 = node.global_basis.z.normalized()
	var up: Vector3 = node.global_basis.y.normalized()
	var distance: float = normal.dot(viewer.global_position - node.global_position)
	if distance <= 0.05: return
	var virtual_eye: Vector3 = viewer.global_position - normal * (2.0 * distance)
	camera.global_position = virtual_eye
	camera.look_at(virtual_eye + normal, up)
	var surface: QuadMesh = node.mesh
	var width: float = surface.size.x * node.global_basis.x.length()
	var height: float = surface.size.y * node.global_basis.y.length()
	if width <= 0.001 or height <= 0.001: return
	var near_plane: float = distance + 0.02
	var ratio: float = near_plane / distance
	camera.near = near_plane
	camera.far = maxf(4000.0, near_plane + 100.0)
	camera.size = height * ratio
	var center_offset: Vector3 = node.global_position - virtual_eye
	camera.frustum_offset = Vector2(center_offset.dot(camera.global_basis.x), center_offset.dot(camera.global_basis.y)) * ratio
	var pixels := Vector2i(512, clampi(int(512.0 * height / width), 128, 1024))
	if viewport.size != pixels: viewport.size = pixels
	viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
