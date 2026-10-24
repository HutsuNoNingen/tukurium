extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func wait_for(test: Callable, reason: String, seconds: float = 25.0) -> bool:
	var deadline := Time.get_ticks_msec() + int(seconds * 1000.0)
	while Time.get_ticks_msec() < deadline:
		if test.call(): return true
		await process_frame
	push_error(reason)
	quit(1)
	return false

func click(panel, text: String) -> void:
	for button in panel.shell.find_children("*", "Button", true, false):
		if button.text == text:
			button.pressed.emit()
			return
	push_error("Button missing: " + text)
	quit(1)

func has_button(panel, text: String) -> bool:
	for button in panel.shell.find_children("*", "Button", true, false):
		if button.text==text: return true
	return false

func screenshot(path: String) -> void:
	if path.is_empty(): return
	await process_frame
	await process_frame
	root.get_texture().get_image().save_png(path)

func run() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await wait_for(func(): return scene.connected and scene.service_panel.enabled, "Lobby not connected"): return
	var panel = scene.service_panel
	if panel.version_label.text != scene.app_version or not panel.version_label.is_visible_in_tree():
		push_error("App version is missing from the visible header")
		quit(1)
		return
	var owner_name := "UI Creator %s" % Time.get_unix_time_from_system()
	panel.name_input.text = owner_name
	click(panel,"Start")
	if not await wait_for(func(): return not panel.current_user.is_empty(), "Name entry failed"): return
	panel.session_name.text = "UI Session"
	click(panel,"作成")
	if not await wait_for(func(): return not panel.current_session.is_empty() and scene.editable and not scene.objects.is_empty(), "Create session failed"): return
	scene.select_object(str(scene.objects.keys()[0]))
	panel.open_button.pressed.emit()
	panel.switch_page("library")
	click(panel,"選択中のアイテムを保存")
	if not await wait_for(func(): return panel.library_total==1, "Save did not appear in the library"):
		print("Library status: ", panel.status.text, " Selected: ", scene.current)
		return
	var screenshot := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="): screenshot = arg.trim_prefix("--screenshot=")
	click(panel,"詳細を見る")
	if not await wait_for(func(): return has_button(panel,"ワークショップに公開…"),"Item details did not load"): return
	click(panel,"ワークショップに公開…")
	panel.publish_title.text="みんなのキューブ"
	panel.publish_description.text="保存して、編集したり別の人に渡したりできるキューブです。"
	if not panel.publish_dialog.find_children("*", "CheckBox", true, false).is_empty():
		push_error("License controls should not be shown")
		quit(1)
		return
	await self.screenshot(screenshot.replace(".png","-publish.png"))
	panel.publish_dialog.confirmed.emit()
	if not await wait_for(func(): return panel.page=="workshop" and panel.workshop_total>0 and has_button(panel,"公開を停止…"),"Publishing UI failed"): return
	var publication: Dictionary=panel.selected.duplicate(true)
	panel.call_service("leave_session",{},func(_v): pass)
	if not await wait_for(func(): return panel.current_session.is_empty(), "Leave session failed"): return
	panel.name_input.text="UI Visitor %s" % Time.get_unix_time_from_system()
	panel.login()
	if not await wait_for(func(): return panel.current_user.get("name","")!=owner_name,"Switching HN failed"): return
	for label in panel.library.find_children("*", "Label", true, false):
		if label.text=="みんなのキューブ": push_error("Previous HN's library remains visible after switching"); quit(1); return
	panel.mine_only.button_pressed=false
	panel.select_item(publication,"workshop")
	if not await wait_for(func(): return has_button(panel,"ライブラリに保存"),"Public item details failed"): return
	await self.screenshot(screenshot)
	click(panel,"ライブラリに保存")
	if not await wait_for(func(): return has_button(panel,"ライブラリに追加済み"),"Workshop acquisition UI failed"): return
	click(panel,"マイライブラリを開く")
	if not await wait_for(func(): return panel.library_total==1,"Acquired item missing in library"): return
	click(panel,"詳細を見る")
	if not await wait_for(func(): return has_button(panel,"ワークショップに公開…"),"Republish action missing"): return
	for button in panel.shell.find_children("*", "Button", true, false):
		if button.text in ["ワークショップに公開…", "ユーザーに直接渡す…"] and button.disabled:
			push_error("Acquired items must allow publishing and sharing")
			quit(1)
			return
	panel.name_input.text=owner_name
	panel.login()
	if not await wait_for(func(): return panel.current_user.get("name","")==owner_name,"Returning to owner HN failed"): return
	panel.switch_page("workshop")
	panel.mine_only.button_pressed=true
	panel.select_item(publication,"workshop")
	if not await wait_for(func(): return has_button(panel,"公開を停止…"),"Owner publication controls missing"): return
	click(panel,"公開を停止…")
	for child in scene.get_children():
		if child is ConfirmationDialog and child.visible: child.confirmed.emit()
	if not await wait_for(func(): return panel.workshop_total==0,"Unpublish UI failed"): return
	print("MANAGED UI SMOKE PASS: save, publish, visitor acquire, owner unpublish")
	quit(0)
