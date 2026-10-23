extends SceneTree
var phases: Dictionary = {}
var badge_seen := false
var screenshot := ""
var captured := false

func observe_status(key: String, state: Dictionary, scene: Node) -> void:
	if not phases.has(key): phases[key] = []
	phases[key].append(state.phase)
	if state.phase == "loading_model" and not screenshot.is_empty() and not captured:
		captured = true
		scene.asset_status.refresh()
		badge_seen = badge_seen or not scene.asset_status.badges.is_empty()
		await RenderingServer.frame_post_draw
		root.get_texture().get_image().save_png(screenshot)

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	var scene: Node = load("res://main.tscn").instantiate()
	root.add_child(scene)
	scene.asset_status.changed.connect(observe_status.bind(scene))
	var fixture := ""
	var expected := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--fixture="): fixture = arg.trim_prefix("--fixture=")
		if arg.begins_with("--asset="): expected = arg.trim_prefix("--asset=")
		if arg.begins_with("--status-capture="): screenshot = arg.trim_prefix("--status-capture=")
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
			badge_seen = badge_seen or not scene.asset_status.badges.is_empty()
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
		var import_phases: Array = []
		for key in phases:
			if str(key).begins_with("import:"): import_phases.append_array(phases[key])
		for phase in ["queued","preparing","uploading_local","uploading_host","placing","ready"]:
			if not import_phases.has(phase): push_error("Missing import phase: "+phase); quit(1); return
	else:
		for phase in ["waiting","downloading_network","downloading_local","verifying","loading_model","ready"]:
			if not phases.get(expected,[]).has(phase): push_error("Missing download phase: "+phase); quit(1); return
		scene.asset_status.refresh()
		if not badge_seen or not scene.asset_status.badges.is_empty(): push_error("Loading badge did not transition to a rendered model"); quit(1); return
		var missing := "0".repeat(64)
		scene.asset_importer.retry_asset(missing)
		# An invalid upload triggers the real engine error path for pending subscribers.
		scene.queue_message({"kind":"asset_offer","asset_id":missing,"size":0})
		for _i in range(300):
			await create_timer(0.01).timeout
			if scene.asset_status.entries.get(missing,{}).get("phase") == "error": break
		scene.asset_status.refresh()
		if not scene.asset_status.row_nodes[missing].retry.visible: push_error("Failed transfer has no Retry action"); quit(1); return
		scene.asset_status.row_nodes[missing].retry.pressed.emit()
		if scene.asset_status.entries[missing].phase != "waiting": push_error("Retry did not request the asset again"); quit(1); return
		if not screenshot.is_empty() and not captured: push_error("Missing loading screenshot"); quit(1); return
	print("ASSET STATUS PASS: import, network/cache download, verification, rendering and retry")
	print("ASSET SMOKE PASS")
	quit(0)
