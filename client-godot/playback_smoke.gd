extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func fail(message: String) -> void:
	push_error(message)
	quit(1)

func run() -> void:
	var scene: Node = load("res://main.tscn").instantiate()
	root.add_child(scene)
	var late := "--late" in OS.get_cmdline_user_args()
	var video_id := ""
	var player: VideoStreamPlayer
	for _i in range(1200):
		await create_timer(0.025).timeout
		for id in scene.objects:
			if scene.objects[id].visual.kind == "video" and scene.render_nodes[id].has_node("VideoPlayer"):
				video_id = id
				player = scene.render_nodes[id].get_node("VideoPlayer")
		if player != null and scene.clock_ready: break
	if player == null or not scene.clock_ready: fail("No video or calibrated host clock"); return
	scene.select_object(video_id)
	var paused_frame := 0
	if not late:
		for _i in range(400):
			await create_timer(0.025).timeout
			if player.paused and absf(player.stream_position-3.25)<0.05: break
		if not player.paused or absf(player.stream_position-3.25)>0.05: fail("Paused seek not applied"); return
		paused_frame = hash(player.get_video_texture().get_image().get_data())
		print("PLAYBACK READY")
	for _i in range(800):
		await create_timer(0.025).timeout
		if not player.paused and scene.objects[video_id].playback.playing: break
	if player.paused: fail("Shared play did not arrive"); return
	# Simulate a decoder stall without any world command; the renderer must repair it.
	player.stream_position += 1.0
	await create_timer(0.6).timeout
	var object: Dictionary = scene.objects[video_id]
	var expected: float = scene.asset_importer.video_position(object,scene.asset_importer.video_duration(object))
	if absf(player.stream_position-expected)>0.28: fail("Renderer did not correct playback drift"); return
	if player.get_video_texture()==null: fail("Video texture missing"); return
	print("PLAYBACK RUNNING "+JSON.stringify({"position":player.stream_position,"expected":expected,"server_time":scene.media_time()}))
	for _i in range(800):
		await create_timer(0.025).timeout
		if player.paused and absf(player.stream_position-5.0)<0.05: break
	if not player.paused or absf(player.stream_position-5.0)>0.05: fail("Shared pause/seek did not arrive"); return
	if absf(scene.video_seek.value-5.0)>0.15: fail("Inspector timeline is stale"); return
	var final_frame := hash(player.get_video_texture().get_image().get_data())
	if DisplayServer.get_name() != "headless" and not late and final_frame == paused_frame: fail("Seek did not change the decoded frame"); return
	print("PLAYBACK FRAME "+str(final_frame))
	print("PLAYBACK SMOKE PASS")
	quit(0)
