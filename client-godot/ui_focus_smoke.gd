extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func key(viewport: Viewport, code: int, pressed: bool) -> void:
	var event := InputEventKey.new()
	event.keycode = code
	event.physical_keycode = code
	event.unicode = code if pressed else 0
	event.pressed = pressed
	viewport.push_input(event, true)

func wait_for(test: Callable, message: String) -> bool:
	for _i in 150:
		await create_timer(0.02).timeout
		if test.call(): return true
	push_error(message)
	return false

func run() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await wait_for(func(): return scene.editable and scene.player_initialized, "Scene not ready"): quit(1); return
	scene.inspector_toggle.button_pressed = true
	# Keep testing keyboard activation after removing the old WORLD sidebar.
	scene.duplicate_button.reparent(scene.name_input.get_parent())
	var object_id: String = scene.selected_id
	for checkbox in [scene.collider_enabled, scene.collider_solid]:
		if not await wait_for(func(): return scene.grounded, "Player did not land"): quit(1); return
		var checked: bool = checkbox.button_pressed
		checkbox.grab_focus()
		key(root, KEY_SPACE, true)
		key(root, KEY_SPACE, false)
		if not await wait_for(func(): return scene.player_target_position.y > 0.15, "Space after checkbox focus did not jump"): quit(1); return
		if checkbox.button_pressed != checked: push_error("Space toggled a checkbox"); quit(1); return
	if not await wait_for(func(): return scene.grounded, "Player did not land before button check"): quit(1); return
	scene.duplicate_button.grab_focus()
	key(root, KEY_SPACE, true)
	key(root, KEY_SPACE, false)
	await create_timer(0.15).timeout
	if scene.objects.size() != 1: push_error("Space activated a focused button"); quit(1); return
	if not await wait_for(func(): return scene.grounded, "Player did not land before typing check"): quit(1); return
	scene.name_input.text = "Typing"
	scene.name_input.grab_focus()
	scene.name_input.caret_column = scene.name_input.text.length()
	Input.action_press("jump")
	key(root, KEY_SPACE, true)
	await create_timer(0.15).timeout
	key(root, KEY_SPACE, false)
	Input.action_release("jump")
	if scene.name_input.text != "Typing ": push_error("Space did not type in LineEdit"); quit(1); return
	if not scene.grounded or scene.player_target_position.y > 0.01: push_error("Typing a space made the player jump"); quit(1); return
	if not await wait_for(func(): return not scene.pending and scene.inspector_edits.is_empty() and not scene.duplicate_button.disabled, "Name edit did not finish"): quit(1); return
	scene.name_input.release_focus()
	scene.duplicate_button.grab_focus()
	key(root, KEY_ENTER, true)
	key(root, KEY_ENTER, false)
	if not await wait_for(func(): return scene.objects.size() == 2, "Enter no longer activates buttons"): quit(1); return
	# A separate script Window also uses ui_accept; Space must not pause a script.
	var editor = scene.script_editor
	editor.open(object_id)
	editor.add_template(1)
	if not await wait_for(func(): return editor.pending_id.is_empty() and not editor.current_script().is_empty(), "Script UI not ready"): quit(1); return
	editor.pause_button.grab_focus()
	var enabled: bool = editor.current_script().enabled
	key(editor.window, KEY_SPACE, true)
	key(editor.window, KEY_SPACE, false)
	await create_timer(0.15).timeout
	if editor.current_script().enabled != enabled: push_error("Space paused a script in a child Window"); quit(1); return
	editor.code.grab_focus()
	editor.code.set_caret_line(0)
	editor.code.set_caret_column(0)
	var source: String = editor.code.text
	Input.action_press("jump")
	key(editor.window, KEY_SPACE, true)
	await create_timer(0.15).timeout
	key(editor.window, KEY_SPACE, false)
	Input.action_release("jump")
	if editor.code.text != " " + source: push_error("Space did not type in CodeEdit"); quit(1); return
	if not scene.grounded: push_error("Typing code made the player jump"); quit(1); return
	print("UI FOCUS SMOKE PASS: Space jumps without activating UI; text entry and Enter still work")
	quit(0)
