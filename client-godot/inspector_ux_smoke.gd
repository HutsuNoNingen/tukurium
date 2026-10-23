extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func until(test: Callable, message: String) -> bool:
	for _i in 600:
		await create_timer(0.025).timeout
		if test.call(): return true
	push_error(message)
	return false

func run() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and not scene.current.is_empty(), "Scene not ready"): quit(1); return
	if scene.inspector_shell.visible: push_error("Legacy inspector should start closed"); quit(1); return
	scene.world_panels.open_inspector(scene.selected_id, true)
	if not await until(func(): return scene.world_panels.windows.size() == 1, "Native inspector did not open"): quit(1); return
	var panel = scene.world_panels.panels.values()[0]
	var view = panel.content
	var window: Window = scene.world_panels.windows[panel.panel_id]
	if not await until(func(): return view.drive_labels.has("rotation") and view.drive_labels.rotation.visible, "Drive provenance is missing"): quit(1); return
	if view.controls.rotation[0].editable: push_error("Driven output should not appear writable"); quit(1); return
	if not view.controls.position[0].editable: push_error("Un-driven field should stay editable"); quit(1); return
	var sections: Array = view.fields_box.get_children().map(func(section): return section.get_child(1).text)
	for expected in ["Slot", "Transform", "MeshRenderer", "Scripts"]:
		if not sections.has(expected): push_error("Missing component section " + expected); quit(1); return
	if not scene.world_panels.has_window_focus() or not scene.gameplay_keyboard_blocked(): push_error("Native focus did not block gameplay"); quit(1); return
	var line: LineEdit = view.controls.name[0]
	line.grab_focus()
	if not await until(func(): return view.edits.has("name") and view.edits.name.granted, "Native field lease not granted"): quit(1); return
	line.select_all()
	for character in "Native shared inspector":
		var event := InputEventKey.new()
		event.keycode = character.unicode_at(0)
		event.unicode = character.unicode_at(0)
		event.pressed = true
		panel.viewport.push_input(event, true)
	if not await until(func(): return scene.world_panels.field_data(view.target_id, "name").value == "Native shared inspector", "Native typing not committed"): quit(1); return
	line.release_focus()
	view.drive_buttons.rotation.pressed.emit()
	if not await until(func(): return view.script_mount.visible and not view.script_view.document.is_empty(), "Driver editor did not open"): quit(1); return
	var driver: Dictionary = scene.world_panels.field_driver(view.target_id, "rotation")
	if view.script_view.editor.selected != driver.id: push_error("Wrong driver selected"); quit(1); return
	view.scripts_button.pressed.emit()
	if not await until(func(): return view.scroll.visible, "Could not return to components"): quit(1); return
	await process_frame
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			panel.viewport.get_texture().get_image().save_png(argument.trim_prefix("--screenshot="))
	window.close_requested.emit()
	if not await until(func(): return scene.world_panels.windows.is_empty(), "Native window did not close"): quit(1); return
	if panel.content != view or panel.viewport.get_parent() != panel: push_error("Close replaced the shared view"); quit(1); return
	scene.world_panels.open_window(panel.panel_id)
	await process_frame
	# Remote deletion must close the local window and dispose its viewport safely.
	scene.world_panels.close_panel(panel.panel_id)
	if not await until(func(): return scene.world_panels.windows.is_empty() and scene.world_panels.panels.is_empty(), "Deleting panel leaked native window"): quit(1); return
	print("INSPECTOR UX SMOKE PASS: component groups, local drive provenance, native input, driver navigation, restore and deletion")
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
