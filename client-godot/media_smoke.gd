extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	var scene: Node = load("res://main.tscn").instantiate()
	root.add_child(scene)
	var fbx := ""
	var video := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--fbx="): fbx = arg.trim_prefix("--fbx=")
		if arg.begins_with("--video="): video = arg.trim_prefix("--video=")
	for _i in range(200):
		await create_timer(0.025).timeout
		if scene.editable: break
	if not scene.editable: push_error("Media client cannot connect"); quit(1); return
	if not fbx.is_empty():
		var before: int = scene.objects.size()
		root.files_dropped.emit(PackedStringArray([fbx,video]))
		for _i in range(14000):
			await create_timer(0.025).timeout
			if scene.asset_importer.active.is_empty() and scene.asset_importer.jobs.is_empty(): break
		if scene.objects.size()!=before+2: push_error("FBX/video import failed: "+scene.feedback.text); quit(1); return
	var video_node: MeshInstance3D
	var model_node: MeshInstance3D
	for _i in range(2000):
		for id in scene.objects:
			if scene.objects[id].visual.kind == "video" and scene.render_nodes[id].has_node("VideoPlayer"): video_node=scene.render_nodes[id]
			if scene.objects[id].visual.kind == "model" and scene.render_nodes[id].has_node("ImportedModel"): model_node=scene.render_nodes[id]
		if video_node!=null and model_node!=null and scene.clock_ready: break
		await create_timer(0.025).timeout
	if video_node==null or model_node==null: push_error("Media assets did not render"); quit(1); return
	var player: VideoStreamPlayer = video_node.get_node("VideoPlayer")
	await create_timer(0.25).timeout
	if not player.is_playing() or player.stream_position<=0.01 or player.get_video_texture()==null:
		push_error("Video did not decode and play"); quit(1); return
	var position := player.stream_position
	await create_timer(0.1).timeout
	var elapsed := fposmod(player.stream_position-position,player.get_stream_length())
	if elapsed<0.03 or elapsed>0.4: push_error("Video time is not advancing"); quit(1); return
	print("MEDIA SMOKE PASS")
	quit(0)
