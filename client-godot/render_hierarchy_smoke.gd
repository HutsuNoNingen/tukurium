extends SceneTree
const HIERARCHY = preload("res://render_hierarchy.gd")

func object(id: String, parent: String, position: Array, rotation: Array = [0,0,0,1], scale: Array = [1,1,1]) -> Dictionary:
	return {"address":{"instance_id":id,"local_id":1}, "parent":null if parent.is_empty() else {"instance_id":parent,"local_id":1}, "object_revision":"1", "fields":[{"id":"position","value":position},{"id":"rotation","value":rotation},{"id":"scale","value":scale}]}

func check_pose(child: Node3D, parent: Node3D, position: Vector3, rotation: Quaternion, scale: Vector3) -> void:
	assert(child.position.distance_to(parent.position + parent.quaternion * (parent.scale * position)) < 0.0001, "Child lags behind rendered parent")
	assert(child.quaternion.is_equal_approx((parent.quaternion * rotation).normalized()), "Child rotation differs")
	assert(child.scale.is_equal_approx(parent.scale * scale), "Child scale differs")

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	# Deliberately shuffled order; include a rotated child under nonuniform scale.
	var turn := Quaternion(Vector3.UP, PI / 3)
	var q := [turn.x,turn.y,turn.z,turn.w]
	var objects := {"leaf":object("leaf","child",[0,1,0]), "child":object("child","parent",[2,0,0],q,[0.5,2,1]), "parent":object("parent","",[0,0,0])}
	var nodes := {}
	for id in objects:
		nodes[id] = Node3D.new()
		root.add_child(nodes[id])
	var hierarchy = HIERARCHY.new()
	hierarchy.sync(objects,{})
	var preview := {"parent":{"position":[8,2,-3],"rotation":q,"scale":[2,3,0.5]}}
	for fps in [30,60,144]:
		hierarchy.draw(nodes,{},"",Vector3.ZERO,0)
		for frame in fps:
			# Simulate new engine frames while display interpolation is unfinished.
			if frame % 3 == 0: hierarchy.sync(objects,{})
			hierarchy.draw(nodes,preview,"",Vector3.ZERO,1.0/fps)
			check_pose(nodes.child,nodes.parent,Vector3(2,0,0),turn,Vector3(0.5,2,1))
			check_pose(nodes.leaf,nodes.child,Vector3(0,1,0),Quaternion.IDENTITY,Vector3.ONE)
	# A child held in world space becomes the rendered parent of its own subtree.
	preview.child = {"position":[-4,2,1],"rotation":[0,0,0,1],"scale":[1,1,1]}
	hierarchy.draw(nodes,preview,"",Vector3.ZERO,1.0/60)
	check_pose(nodes.leaf,nodes.child,Vector3(0,1,0),Quaternion.IDENTITY,Vector3.ONE)
	# A local gizmo must move descendants immediately, without an engine round trip.
	hierarchy.draw(nodes,{},"parent",Vector3(3,4,5),0)
	check_pose(nodes.child,nodes.parent,Vector3(2,0,0),turn,Vector3(0.5,2,1))
	# Drive fields affect the hierarchy; stale frames cannot undo a committed edit.
	var driven := {"parent":{"target":objects.parent.address,"parent":null,"object_revision":"1","local_position":[10,0,0],"local_rotation":q,"local_scale":[2,2,2]}}
	hierarchy.sync(objects,driven)
	hierarchy.draw(nodes,{},"",Vector3.ZERO,0)
	assert(nodes.parent.position == Vector3(10,0,0))
	check_pose(nodes.child,nodes.parent,Vector3(2,0,0),turn,Vector3(0.5,2,1))
	objects.parent.object_revision = "2"
	hierarchy.sync(objects,driven)
	hierarchy.draw(nodes,{},"",Vector3.ZERO,0)
	assert(nodes.parent.position == Vector3.ZERO, "Old frame survived an edit")
	# Reparent and subtree removal leave no stale inherited pose.
	objects.child.parent = null
	hierarchy.sync(objects,{})
	hierarchy.draw(nodes,{},"",Vector3.ZERO,0)
	assert(nodes.child.position == Vector3(2,0,0))
	check_pose(nodes.leaf,nodes.child,Vector3(0,1,0),Quaternion.IDENTITY,Vector3.ONE)
	objects.erase("child")
	objects.erase("leaf")
	hierarchy.sync(objects,{})
	assert(hierarchy.order == ["parent"])
	for node in nodes.values(): node.free()
	print("RENDER HIERARCHY SMOKE PASS: 30/60/144 FPS, nested previews, rotation/scale, Drive, gizmo, reparent, stale frames, removal")
	quit()
