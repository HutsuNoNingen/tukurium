extends SceneTree
var saw_upload := false
var saw_placeholder := false

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	var scene: Node = load("res://main.tscn").instantiate()
	root.add_child(scene)
	scene.asset_status.changed.connect(func(_key, state):
		if state.phase == "waiting_upload": saw_upload = true)
	for _i in range(500):
		await create_timer(0.01).timeout
		if scene.editable: break
	print("ASSET OBSERVER READY")
	var target := ""
	var revision := ""
	for _i in range(15000):
		await create_timer(0.01).timeout
		for object in scene.objects.values():
			if object.visual.kind != "model": continue
			target = object.address.instance_id
			var node: Node = scene.render_nodes[target]
			if not node.has_node("ImportedModel"):
				saw_placeholder = true
				revision = object.object_revision
			else:
				if not saw_placeholder or not saw_upload: push_error("No early Slot or upload progress"); quit(1); return
				if object.object_revision != revision: push_error("Loading bytes changed persistent object state"); quit(1); return
				print("ASSET OBSERVER PASS: Slot is shared during upload; same object later renders its asset")
				quit(0)
				return
	push_error("Asset observer timed out")
	quit(1)
