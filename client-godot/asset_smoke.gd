extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	var scene: Node = load("res://main.tscn").instantiate()
	root.add_child(scene)
	var fixture := ""
	var expected := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--fixture="): fixture = arg.trim_prefix("--fixture=")
		if arg.begins_with("--asset="): expected = arg.trim_prefix("--asset=")
	for _i in range(200):
		await create_timer(0.025).timeout
		if scene.editable: break
	if not scene.editable: push_error("Asset test could not connect"); quit(1); return
	if not fixture.is_empty():
		var img := Image.create(32,16,false,Image.FORMAT_RGBA8)
		img.fill(Color(1,0.5,0.1,1))
		var png := fixture.get_base_dir().path_join("drop-image.png")
		img.save_png(png)
		var before: int = scene.objects.size()
		# Exercise the same signal used by Explorer, including a queued multi-file drop.
		root.files_dropped.emit(PackedStringArray([png,fixture]))
		for _i in range(14000):
			await create_timer(0.025).timeout
			if scene.objects.size()==before+2 and scene.asset_importer.active.is_empty(): break
		if scene.objects.size()!=before+2:
			push_error("D&D did not import image and model: "+scene.feedback.text); quit(1); return
		for id in scene.objects:
			if scene.objects[id].visual.kind == "model":
				expected = scene.objects[id].visual.asset_id
				if scene.objects[id].collider == null or not scene.objects[id].collider.solid:
					push_error("Model has no shared solid collider"); quit(1); return
		print("IMPORTED_ASSET "+expected)
	else:
		for _i in range(14000):
			await create_timer(0.025).timeout
			if scene.asset_importer.templates.has(expected): break
	if expected.is_empty() or not scene.asset_importer.templates.has(expected):
		push_error("Model bytes did not reach renderer"); quit(1); return
	var rendered := false
	for id in scene.objects:
		if scene.objects[id].visual.get("asset_id","")==expected:
			var node: Node = scene.render_nodes[id]
			if node.has_node("ImportedModel"):
				var meshes := node.get_node("ImportedModel").find_children("*","MeshInstance3D",true,false)
				rendered = meshes.size()>0
	if not rendered: push_error("Shared GLB did not render"); quit(1); return
	if not fixture.is_empty():
		var revision: String = scene.current.object_revision
		scene.scale_inputs[0].value = 1.5
		for _i in range(150):
			await create_timer(0.025).timeout
			if scene.current.object_revision != revision: break
		if scene.current.world_scale[0] != 1.5: push_error("Model scale did not synchronize"); quit(1); return
	print("ASSET SMOKE PASS")
	quit(0)
