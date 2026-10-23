extends RefCounted

# A local presentation copy. Rust owns parenting, local fields and physics.
# Keep meshes flat for picking/UI, but resolve their poses as one ordered tree.
var poses: Dictionary = {}
var order: Array[String] = []

func sync(objects: Dictionary, evaluated: Dictionary) -> void:
	poses.clear()
	order.clear()
	for id: String in objects:
		var object: Dictionary = objects[id]
		var fields := {}
		for field in object.fields: fields[field.id] = field.value
		var frame: Dictionary = evaluated.get(id, {})
		# A queued frame must not undo a newer reparent, edit or restored snapshot.
		if frame.get("target") != object.address or frame.get("parent") != object.get("parent") or frame.get("object_revision") != object.object_revision:
			frame = {}
		poses[id] = {
			"parent": str(object.parent.instance_id) if object.get("parent") is Dictionary else "",
			"position": vector(frame.get("local_position", fields.get("position", [0,0,0]))),
			"rotation": quaternion(frame.get("local_rotation", fields.get("rotation", [0,0,0,1]))),
			"scale": vector(frame.get("local_scale", fields.get("scale", [1,1,1])))
		}
	var pending := poses.keys()
	while not pending.is_empty():
		var progressed := false
		for id: String in pending.duplicate():
			var parent: String = poses[id].parent
			if not poses.has(parent) or order.has(parent):
				order.append(id)
				pending.erase(id)
				progressed = true
		if not progressed: break # Rust rejects cycles; don't hang on incomplete input.

func draw(nodes: Dictionary, previews: Dictionary, gizmo_id: String, gizmo_position: Vector3, delta: float) -> void:
	var displayed := {}
	var alpha := 1.0 - exp(-20.0 * delta)
	for id in order:
		if not nodes.has(id): continue
		var local: Dictionary = poses[id]
		var p: Vector3 = local.position
		var q: Quaternion = local.rotation
		var s: Vector3 = local.scale
		var parent: Dictionary = displayed.get(local.parent, {})
		if not parent.is_empty():
			# Match Rust's TRS composition (component scale, without affine shear).
			p = parent.position + parent.rotation * (parent.scale * p)
			q = (parent.rotation * q).normalized()
			s = parent.scale * s
		var node: Node3D = nodes[id]
		if id == gizmo_id:
			p = gizmo_position
		elif previews.has(id):
			var preview: Dictionary = previews[id]
			p = node.position.lerp(vector(preview.position), alpha)
			var target_q: Quaternion = quaternion(preview.rotation) if preview.get("rotation") != null else q
			var target_s: Vector3 = vector(preview.scale) if preview.get("scale") != null else s
			q = node.quaternion.slerp(target_q, alpha)
			s = node.scale.lerp(target_s, alpha)
		node.position = p
		node.quaternion = q
		node.scale = s
		# Descendants use precisely the pose drawn for this parent in this frame.
		displayed[id] = {"position":p, "rotation":q, "scale":s}

static func vector(value: Array) -> Vector3:
	return Vector3(value[0], value[1], value[2])

static func quaternion(value: Array) -> Quaternion:
	return Quaternion(value[0], value[1], value[2], value[3])
