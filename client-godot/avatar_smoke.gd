extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func fail(message: String) -> void:
	push_error(message)
	quit(1)

func run() -> void:
	var side := "observer"
	var signals := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--side="): side = arg.trim_prefix("--side=")
		if arg.begins_with("--signals="): signals = arg.trim_prefix("--signals=")
	var scene: Node = load("res://main.tscn").instantiate()
	scene.avatar_restore_started = true
	root.add_child(scene)
	for _i in range(800):
		await create_timer(0.025).timeout
		if scene.editable and scene.avatars.size()>=2: break
	if not scene.editable: fail("Auto role did not produce an editable world"); return
	print("AVATAR READY")
	if side == "owner":
		for _i in range(800):
			if FileAccess.file_exists(signals.path_join("change")): break
			await create_timer(0.025).timeout
		var image := Image.create(32,32,false,Image.FORMAT_RGBA8)
		image.fill(Color(0.8,0.15,0.6,1))
		var path := signals.path_join("custom.png")
		image.save_png(path)
		scene.asset_importer.queue_avatar_texture(path)
	var target := ""
	for _i in range(1200):
		await create_timer(0.025).timeout
		for id in scene.avatars:
			if (id=="local") != (side=="owner"): continue
			if not scene.avatars[id].texture_id.is_empty(): target=id; break
		if not target.is_empty(): break
	if target.is_empty(): fail("Avatar texture was not replicated"); return
	var avatar: Node = scene.avatars[target]
	for _i in range(800):
		if scene.asset_importer.avatar_textures.has(avatar.texture_id): break
		await create_timer(0.025).timeout
	if not scene.asset_importer.avatar_textures.has(avatar.texture_id): fail("Avatar PNG was not downloaded"); return
	var texture: Texture2D = scene.asset_importer.avatar_textures[avatar.texture_id]
	var pixel := texture.get_image().get_pixel(0,0)
	if absf(pixel.r-0.8)>0.02 or absf(pixel.b-0.6)>0.02 or avatar.paint_material.albedo_texture!=texture:
		fail("Avatar material did not use the shared PNG"); return
	print("AVATAR MATCH "+avatar.texture_id)
	if side=="owner":
		for _i in range(1200):
			if FileAccess.file_exists(signals.path_join("reset")): break
			await create_timer(0.025).timeout
		scene.asset_importer.reset_avatar_texture()
	for _i in range(1000):
		await create_timer(0.025).timeout
		if scene.avatars.has(target) and scene.avatars[target].texture_id.is_empty():
			print("AVATAR SMOKE PASS")
			quit(0)
			return
	fail("Avatar reset was not replicated")
