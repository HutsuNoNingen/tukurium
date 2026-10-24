extends SceneTree

func _initialize() -> void: call_deferred("run")

func until(test: Callable, message: String) -> bool:
	for _i in 600:
		await create_timer(0.025).timeout
		if test.call(): return true
	push_error(message)
	return false

func key(code: int) -> void:
	var event := InputEventKey.new()
	event.keycode=code; event.physical_keycode=code; event.pressed=true
	root.push_input(event,true)
	event=event.duplicate(); event.pressed=false
	root.push_input(event,true)

func click(at: Vector2, button: int = MOUSE_BUTTON_LEFT) -> void:
	var motion := InputEventMouseMotion.new()
	motion.position=at
	root.push_input(motion,true)
	var event := InputEventMouseButton.new()
	event.button_index=button; event.position=at; event.global_position=at; event.pressed=true
	root.push_input(event,true)
	event=event.duplicate(); event.pressed=false
	root.push_input(event,true)

func drag(from: Vector2, to: Vector2) -> void:
	var down := InputEventMouseButton.new()
	down.button_index=MOUSE_BUTTON_LEFT; down.position=from; down.global_position=from; down.pressed=true
	root.push_input(down,true)
	var motion := InputEventMouseMotion.new()
	motion.position=to; motion.global_position=to; motion.relative=to-from
	root.push_input(motion,true)
	var up := down.duplicate()
	up.position=to; up.global_position=to; up.pressed=false
	root.push_input(up,true)

func capture(suffix: String) -> void:
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await process_frame
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").replace(".png",suffix+".png"))

func run() -> void:
	var scene=load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and scene.player_initialized,"Scene not ready"): quit(1); return
	if not scene.first_person or scene.view_mode.selected!=1: push_error("Default view is not FPS"); quit(1); return
	if scene.fps_active or Input.mouse_mode!=Input.MOUSE_MODE_VISIBLE: push_error("Startup captured the mouse"); quit(1); return
	click(root.size*0.5)
	if not scene.fps_active or Input.mouse_mode!=Input.MOUSE_MODE_CAPTURED: push_error("Clicking the world did not start FPS controls"); quit(1); return
	if scene.find_child("LegacyWorldSidebar",true,false).is_visible_in_tree(): push_error("Old world sidebar is visible"); quit(1); return
	if not await until(func(): return scene.hud.sync_label.text.contains("同期済み") and scene.hud.fps_label.text.contains("FPS"),"HUD status missing"): quit(1); return
	await create_timer(1.1).timeout
	await capture("")
	key(KEY_F5)
	if scene.view_mode.selected!=2: push_error("F5 did not switch to fly view"); quit(1); return
	key(KEY_F5); key(KEY_F5)
	if not scene.first_person: push_error("F5 did not cycle back to FPS"); quit(1); return
	key(KEY_ESCAPE)
	if scene.hud.menu.visible or scene.fps_active or Input.mouse_mode!=Input.MOUSE_MODE_VISIBLE: push_error("Esc did not release cursor without opening menu"); quit(1); return
	if not scene.hud.dock_buttons.has("sessions") or not scene.hud.dock_buttons.has("library") or not scene.hud.dock_buttons.has("workshop") or not scene.hud.dock_buttons.has("settings"):
		push_error("Separate dock icons missing"); quit(1); return
	click(scene.hud.dock_buttons.settings.get_global_rect().get_center())
	if not scene.hud.menu.visible or not scene.gameplay_keyboard_blocked(): push_error("Settings dock did not open its window"); quit(1); return
	if not scene.hud.room_label.is_visible_in_tree(): push_error("Top bar disappeared behind settings"); quit(1); return
	if not scene.nickname.is_visible_in_tree() or not scene.avatar_button.is_visible_in_tree(): push_error("User settings are not in the menu"); quit(1); return
	var position: Vector3=scene.player_target_position
	Input.action_press("move_forward"); Input.action_press("jump")
	await create_timer(0.2).timeout
	Input.action_release("move_forward"); Input.action_release("jump")
	if scene.player_target_position.distance_to(position)>0.02: push_error("Menu input moved the player"); quit(1); return
	await capture("-menu")
	key(KEY_ESCAPE)
	if scene.hud.menu.visible or scene.fps_active: push_error("Esc did not close menu and leave cursor visible"); quit(1); return
	# The connected personal surfaces use the separate dock buttons.
	var workspace=scene.get("service_panel")
	if workspace != null:
		workspace.enabled=true
		await until(func(): return not scene.hud.dock_buttons.sessions.disabled,"Session dock stayed disabled")
		click(scene.hud.dock_buttons.sessions.get_global_rect().get_center())
		if not workspace.shade.visible or workspace.page!="sessions" or not scene.gameplay_keyboard_blocked(): push_error("Session window was not opened"); quit(1); return
		if not scene.hud.room_label.is_visible_in_tree() or workspace.floating_window.size.x>=root.size.x: push_error("Top bar or floating window layout missing"); quit(1); return
		await capture("-sessions")
		var before: Vector2=workspace.floating_window.position
		drag(workspace.heading.get_global_rect().get_center(),workspace.heading.get_global_rect().get_center()+Vector2(38,24))
		if workspace.floating_window.position.distance_to(before)<10: push_error("Session window did not drag"); quit(1); return
		var old_size: Vector2=workspace.floating_window.size
		drag(workspace.floating_window.grip.get_global_rect().get_center(),workspace.floating_window.grip.get_global_rect().get_center()+Vector2(-40,-32))
		if workspace.floating_window.size.distance_to(old_size)<10: push_error("Session window did not resize"); quit(1); return
		click(scene.hud.dock_buttons.library.get_global_rect().get_center())
		if workspace.page!="library" or not workspace.shade.visible: push_error("Inventory dock did not open library"); quit(1); return
		click(scene.hud.dock_buttons.workshop.get_global_rect().get_center())
		if workspace.page!="workshop" or not workspace.shade.visible: push_error("Workshop dock did not open workshop"); quit(1); return
		key(KEY_ESCAPE)
		if workspace.shade.visible or scene.fps_active: push_error("Esc did not close floating window and release cursor"); quit(1); return
		workspace.enabled=false
		scene.hud.resume_view()
	click(root.size*0.5,MOUSE_BUTTON_MIDDLE)
	if not scene.hud.ring.visible or not scene.gameplay_keyboard_blocked(): push_error("Middle click did not open ring"); quit(1); return
	await capture("-ring")
	click(scene.hud.ring.center+Vector2(0,-125))
	if not await until(func(): return scene.world_panels.windows.size()==1,"Ring did not open local inspector"): quit(1); return
	if not scene.world_panels.panels.is_empty(): push_error("2D ring action created shared panel"); quit(1); return
	scene.world_panels.close_window(scene.world_panels.windows.keys()[0])
	await create_timer(0.1).timeout
	scene.hud.resume_view()
	click(root.size*0.5,MOUSE_BUTTON_MIDDLE)
	click(scene.hud.ring.center+Vector2(125,0))
	if not await until(func(): return scene.world_panels.panels.size()==1,"Ring did not open shared inspector"): quit(1); return
	if not scene.world_panels.windows.is_empty(): push_error("3D ring action opened local window"); quit(1); return
	click(root.size*0.5,MOUSE_BUTTON_MIDDLE)
	key(KEY_ESCAPE)
	if scene.hud.ring.visible or scene.hud.menu.visible: push_error("Esc did not cancel ring"); quit(1); return
	print("HUD SMOKE PASS: cursor-first FPS, top bar, dock, floating window drag/resize, Esc, blocked movement, status and ring 2D/3D")
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
