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
	if not await until(func(): return scene.editable and not scene.current.is_empty(),"Scene not ready"): quit(1); return
	var target: String = scene.selected_id
	for _i in 2: scene.world_panels.open_inspector(target)
	if not await until(func(): return scene.world_panels.panels.size() == 2,"Shared inspectors missing"): quit(1); return
	var panels: Array = scene.world_panels.panels.values()
	for panel in panels: scene.world_panels.edit(panel.panel_id,"inspector_scripts_open",true)
	if not await until(func(): return panels[0].content.script_mount.visible and panels[1].content.script_mount.visible,"Scripts tabs did not sync"): quit(1); return
	var a = panels[0].content.script_view
	var b = panels[1].content.script_view
	if not await until(func(): return not a.document.is_empty() and not b.document.is_empty(),"Documents did not open"): quit(1); return
	var original: String = a.editor.code.text
	a.editor.code.text = "-- Alice\n" + original
	a.editor.code.text_changed.emit()
	b.editor.code.text = "-- Bob\n" + original
	b.editor.code.text_changed.emit()
	a.flush()
	b.flush()
	# Type again while the first request is still awaiting acknowledgement.
	a.editor.code.text += "\n-- more from Alice\n"
	a.editor.code.text_changed.emit()
	if not await until(func(): return a.flight.is_empty() and b.flight.is_empty() and a.editor.code.text == b.editor.code.text and a.editor.code.text.contains("-- Alice") and a.editor.code.text.contains("-- Bob") and a.editor.code.text.contains("-- more from Alice"),"Concurrent edits did not converge"): quit(1); return
	if scene.objects[target].scripts[0].source == a.editor.code.text:
		push_error("Draft was executed before Apply"); quit(1); return
	a.editor.code.text += "\n-- Apply while synchronizing\n"
	a.editor.code.text_changed.emit()
	a.flush()
	a.editor.apply_button.pressed.emit()
	if not await until(func(): return a.editor.pending_id.is_empty() and a.pending_action.is_empty() and scene.objects[target].scripts[0].source == a.editor.code.text,"Apply did not compile the shared draft"): quit(1); return
	if not await until(func(): return a.document.get("cursors",[]).size() >= 2,"Multiple cursors not visible"): quit(1); return
	var id: String = a.editor.selected
	a.editor.code.text += "\n-- syntax stays in the shared draft: (\n"
	a.editor.code.text_changed.emit()
	if not await until(func(): return a.flight.is_empty() and a.editor.code.text == b.editor.code.text,"Draft tail did not replicate"): quit(1); return
	# A comment containing an unmatched parenthesis remains valid; now test invalid source.
	var valid: String = scene.objects[target].scripts[0].source
	a.editor.code.text += "return ("
	a.editor.code.text_changed.emit()
	if not await until(func(): return a.flight.is_empty() and a.editor.code.text == b.editor.code.text,"Invalid draft not shared"): quit(1); return
	a.editor.apply_button.pressed.emit()
	if not await until(func(): return a.editor.pending_id.is_empty(),"Apply error response missing"): quit(1); return
	if scene.objects[target].scripts[0].source != valid:
		push_error("Invalid draft replaced running code"); quit(1); return
	# Restore the shared draft, then capture the real world-panel viewport.
	a.editor.code.text = valid
	a.editor.code.text_changed.emit()
	if not await until(func(): return a.flight.is_empty() and a.editor.code.text == b.editor.code.text,"Draft restore did not converge"): quit(1); return
	b.request("cursor",{"anchor":b.document.ids[mini(12,b.document.ids.size()-1)],"head":b.document.ids[mini(12,b.document.ids.size()-1)],"name":"Bob"},func(_r): pass)
	await create_timer(0.2).timeout
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			panels[0].viewport.get_texture().get_image().save_png(arg.trim_prefix("--screenshot="))
	print("SHARED SCRIPT SMOKE PASS: concurrent drafts, multiple cursors, Apply and syntax errors")
	quit(0)
