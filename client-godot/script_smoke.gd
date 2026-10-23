extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func check_until(test: Callable, message: String, count: int = 300) -> bool:
	for _i in count:
		await create_timer(0.025).timeout
		if test.call(): return true
	push_error(message)
	return false

func run() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await check_until(func(): return scene.editable and not scene.current.is_empty(), "UGC scene not connected"): quit(1); return
	var id: String = scene.selected_id
	var editor = scene.script_editor
	editor.open(id)
	editor.add_template(0)
	if not await check_until(func(): return scene.objects[id].get("scripts", []).size() == 1 and editor.pending_id.is_empty(), "Add script failed"):
		print("EDITOR DEBUG ", editor.feedback.text, " pending=", editor.pending_id, " templates=", scene.script_templates.size(), " world=", scene.objects[id].get("scripts", []), " outgoing=",scene.outgoing.size()," frames=",scene.frames," connected=",scene.connected)
		quit(1); return
	var first_rotation: Array = scene.objects[id].world_rotation.duplicate()
	if not await check_until(func(): return scene.objects[id].world_rotation != first_rotation, "Luau did not rotate object"): quit(1); return
	editor.code.text = editor.code.text.replace("default = 45", "default = 90")
	editor.apply_button.pressed.emit()
	if not await check_until(func(): return scene.objects[id].scripts[0].parameters.speed == 90 and editor.pending_id.is_empty(), "Edited Luau did not execute"):
		print("EDIT DEBUG ",editor.feedback.text," pending=",editor.pending_id," component=",scene.objects[id].scripts[0])
		quit(1); return
	editor.parameter_controls.speed.value = 120
	if not await check_until(func(): return scene.objects[id].scripts[0].parameters.speed == 120 and editor.pending_id.is_empty(), "Public field did not auto-sync"): quit(1); return
	editor.enabled.button_pressed = false
	if not await check_until(func(): return not scene.objects[id].scripts[0].enabled and editor.pending_id.is_empty(), "Pause script failed"): quit(1); return
	await create_timer(0.15).timeout
	first_rotation = scene.objects[id].world_rotation.duplicate()
	await create_timer(0.2).timeout
	if scene.objects[id].world_rotation != first_rotation: push_error("Disabled script still runs"); quit(1); return
	editor.add_template(1)
	if not await check_until(func(): return scene.objects[id].scripts.size() == 2 and editor.pending_id.is_empty(), "Second script missing"): quit(1); return
	editor.click_button.pressed.emit()
	if not await check_until(func(): return scene.objects[id].scripts[1].state.get("active", false), "Click callback did not run"): quit(1); return
	var tint: Color = scene.render_nodes[id].material_override.albedo_color
	if not tint.is_equal_approx(Color(1, 0.3, 0.1, 1)): push_error("Click color did not reach renderer"); quit(1); return
	scene.gizmo.clear_target()
	var click := InputEventMouseButton.new()
	click.button_index = MOUSE_BUTTON_LEFT
	click.pressed = true
	click.position = scene.camera.unproject_position(scene.render_nodes[id].global_position)
	scene._unhandled_input(click)
	if not await check_until(func(): return not scene.objects[id].scripts[1].state.get("active", true), "World click did not invoke script"): quit(1); return
	editor.click_button.pressed.emit()
	if not await check_until(func(): return scene.objects[id].scripts[1].state.get("active", false), "Second click did not run"): quit(1); return
	var valid_source: String = scene.objects[id].scripts[1].source
	editor.code.text = "return { invalid source"
	editor.apply_button.pressed.emit()
	if not await check_until(func(): return editor.pending_id.is_empty(), "Invalid code response missing"): quit(1); return
	if scene.objects[id].scripts[1].source != valid_source: push_error("Invalid code replaced working script"); quit(1); return
	editor.reload_button.pressed.emit()
	# Own scripts can be added alongside the two templates, then independently selected.
	for count in [3,4]:
		editor.new_button.pressed.emit()
		if not await check_until(func(): return scene.objects[id].scripts.size() == count and editor.pending_id.is_empty(), "Blank script was not attached"): quit(1); return
	if editor.current_script().schema.name != "Script2" or editor.component_count.text.find("4 / 4") < 0:
		push_error("Attached script count or unique blank names are wrong"); quit(1); return
	if not editor.new_button.disabled or not editor.attach_button.disabled:
		push_error("Script limit is not shown by add controls"); quit(1); return
	editor.scripts.item_selected.emit(0)
	if editor.current_script().schema.name != "Rotator" or editor.code.text.find('name = "Rotator"') < 0:
		push_error("Selecting an attached script did not change the editor"); quit(1); return
	for index in [3,2]:
		editor.scripts.item_selected.emit(index)
		editor.remove_button.pressed.emit()
		if not editor.remove_dialog.visible: push_error("Removal has no confirmation"); quit(1); return
		editor.remove_dialog.hide()
		editor.remove_dialog.confirmed.emit()
		if not await check_until(func(): return scene.objects[id].scripts.size() == index and editor.pending_id.is_empty(), "Removing a selected blank script failed"): quit(1); return
	if editor.new_button.disabled: push_error("Adding remains disabled below the limit"); quit(1); return
	editor.scripts.item_selected.emit(1)
	var screenshot := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="): screenshot = arg.trim_prefix("--screenshot=")
	if not screenshot.is_empty():
		await create_timer(0.3).timeout
		var image: Image = editor.window.get_texture().get_image()
		if image.is_empty(): push_error("No script editor screenshot"); quit(1); return
		image.save_png(screenshot)
	print("SCRIPT EDITOR SMOKE PASS")
	quit(0)
