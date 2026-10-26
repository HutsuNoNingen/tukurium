extends SceneTree
## Exercise the settings input path and a fresh process's saved preference.

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

func click(at: Vector2) -> void:
	var motion := InputEventMouseMotion.new()
	motion.position=at; motion.global_position=at
	root.push_input(motion,true)
	var event := InputEventMouseButton.new()
	event.button_index=MOUSE_BUTTON_LEFT; event.position=at; event.global_position=at; event.pressed=true
	root.push_input(event,true)
	event=event.duplicate(); event.pressed=false
	root.push_input(event,true)

func option_key(selector: OptionButton, korean: bool) -> void:
	click(selector.get_global_rect().get_center())
	await process_frame
	# PopupMenu receives keyboard navigation, just as an actual dropdown does.
	var popup := selector.get_popup()
	if not popup.visible: push_error("Language dropdown did not open"); return
	var target := 1 if korean else 0
	if DisplayServer.get_name()=="headless":
		popup.activate_item(target)
		return
	var helper := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--locale-input-helper="): helper=arg.trim_prefix("--locale-input-helper=")
	var handle := DisplayServer.window_get_native_handle(DisplayServer.WINDOW_HANDLE,popup.get_window_id())
	var focused := popup.get_focused_item()
	var steps := target+1 if focused<0 else posmod(target-focused,2)
	if steps==0: steps=2
	var output: Array=[]
	var code := OS.execute("powershell.exe",PackedStringArray(["-NoProfile","-ExecutionPolicy","Bypass","-File",helper,"-Handle",str(handle),"-ExpectedProcessId",str(OS.get_process_id()),"-DownSteps",str(steps)]),output,true,false)
	if code!=0: push_error("Native dropdown input failed: "+str(output))
	await create_timer(0.1).timeout

func capture() -> void:
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await process_frame
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot="))

func run() -> void:
	var phase := "select"
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--locale-phase="): phase=arg.trim_prefix("--locale-phase=")
	var language=root.get_node("AppLanguage")
	var scene=load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and scene.player_initialized,"Localization scene did not connect"): quit(1); return
	var expected := "ko" if phase=="restart" else "ja"
	if language.locale!=expected: push_error("Startup locale: expected %s, got %s" % [expected,language.locale]); quit(1); return
	key(KEY_ESCAPE)
	await create_timer(0.35).timeout
	click(scene.hud.dock_buttons.settings.get_global_rect().get_center())
	await process_frame
	await process_frame
	if not scene.hud.menu.visible: push_error("Could not open language settings through dock"); quit(1); return
	var selector: OptionButton=scene.hud.language_selector
	if selector.item_count!=2 or selector.get_item_text(0)!="日本語" or selector.get_item_text(1)!="한국어":
		push_error("Language selector does not identify both languages by native names"); quit(1); return
	if selector.selected!=(1 if expected=="ko" else 0): push_error("Language selector does not reflect startup preference"); quit(1); return
	if phase=="select":
		# Real tool controls exercise locale notifications without introducing
		# a second world participant or changing any shared field.
		var creation=load("res://creation_panel.gd").new()
		root.add_child(creation)
		creation.setup(scene.world_panels,"localization-creation-fixture")
		creation.hide()
		creation.show_selection(1)
		var japanese_creation: String=creation.create_button.text
		var inspector=load("res://desktop_inspector.gd").new()
		root.add_child(inspector)
		inspector.setup(scene.world_panels,"localization-inspector-fixture",true)
		inspector.hide()
		inspector.set_initial_target(scene.objects.keys()[0])
		if not inspector.controls.has("name"): push_error("Inspector draft fixture did not build name field"); quit(1); return
		var draft: LineEdit=inspector.controls.name[0]
		inspector.filling=true
		draft.text="設定 draft"
		draft.caret_column=2
		draft.select(0,2)
		inspector.filling=false
		var binding := Label.new()
		root.add_child(binding)
		language.format_text(binding,"画角 %d°",[65])
		# A later message must supersede any old formatted binding.
		binding.text="newer status"
		# Keep an existing user-authored value identical to a translation key.
		scene.service_panel.current_session={"name":"セッション"}
		scene.nickname.text="設定"
		await option_key(selector,true)
		if not await until(func(): return language.locale=="ko","Selecting Korean did not apply immediately"): quit(1); return
		if creation.create_button.text!=scene.tr("%s を作成") % scene.tr("Sphere") or creation.create_button.text==japanese_creation:
			push_error("Existing creation button did not refresh its translated shape name"); quit(1); return
		if binding.text!="newer status": push_error("Locale switch restored a superseded formatted message"); quit(1); return
		if inspector.controls.name[0]!=draft or draft.text!="設定 draft" or draft.caret_column!=2 or draft.get_selection_from_column()!=0 or draft.get_selection_to_column()!=2:
			push_error("Locale switch rebuilt or overwrote an inspector draft/caret/selection"); quit(1); return
		language.set_locale("ja")
		if creation.create_button.text!=japanese_creation or draft.text!="設定 draft" or binding.text!="newer status":
			push_error("Returning to Japanese did not preserve tool state"); quit(1); return
		language.set_locale("ko")
		for source in ["ユーザー設定","表示名","参加者","チャット","新規作成"]:
			if scene.tr(source)==source: push_error("Missing Korean translation: "+source); quit(1); return
		if not scene.hud.dock_buttons.settings.tr(scene.hud.dock_buttons.settings.text).contains("설정"):
			push_error("Existing settings dock did not translate to Korean"); quit(1); return
		if scene.nickname.text!="設定": push_error("Language selection changed the user's display-name draft"); quit(1); return
		if not await until(func(): return scene.hud.room_label.text=="セッション","Session title did not refresh"): quit(1); return
		if scene.hud.room_label.auto_translate_mode!=Node.AUTO_TRANSLATE_MODE_DISABLED:
			push_error("User session title is still automatically translated"); quit(1); return
		await capture()
		# Chat travels through the Engine and remains authored content in its view.
		key(KEY_ESCAPE)
		await process_frame
		scene.social_panels.chat_input.text="設定"
		click(scene.social_panels.send_button.get_global_rect().get_center())
		if not await until(func(): return not scene.social_panels.messages.is_empty(),"Japanese chat text did not return from the Engine"): quit(1); return
		if scene.social_panels.messages[-1].text!="設定": push_error("Language selection changed shared chat content"); quit(1); return
		var authored_label: Label
		for control in scene.social_panels.chat_rows.find_children("*","Label",true,false):
			if control.text=="設定": authored_label=control
		if authored_label==null or authored_label.auto_translate_mode!=Node.AUTO_TRANSLATE_MODE_DISABLED:
			push_error("User chat content is still automatically translated"); quit(1); return
		creation.queue_free()
		inspector.queue_free()
		binding.queue_free()
	elif phase=="restart":
		if scene.tr("ユーザー設定")=="ユーザー設定": push_error("Restart did not restore Korean translations"); quit(1); return
		await option_key(selector,false)
		if not await until(func(): return language.locale=="ja","Selecting Japanese did not apply immediately"): quit(1); return
		if scene.tr("ユーザー設定")!="ユーザー設定": push_error("Japanese selection did not restore Japanese labels"); quit(1); return
		if language.set_locale("unsupported"): push_error("Unsupported language was accepted"); quit(1); return
		if language.locale!="ja": push_error("Unsupported language changed the active locale"); quit(1); return
	print("LOCALIZATION SMOKE PASS: "+phase)
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
