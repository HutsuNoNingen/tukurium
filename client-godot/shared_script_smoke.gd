extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func until(test: Callable, message: String) -> bool:
	for _i in 600:
		await create_timer(0.025).timeout
		if test.call(): return true
	push_error(message)
	return false

func mouse(button: int, pressed: bool, point: Vector2) -> void:
	var event := InputEventMouseButton.new()
	event.position = point
	event.button_index = button
	event.pressed = pressed
	root.push_input(event,true)

func fps_click(scene, panel, control: Control) -> void:
	await process_frame
	var point := control.get_global_rect().get_center()
	var local := Vector3((point.x/panel.viewport.size.x-0.5)*panel.mesh.size.x,(0.5-point.y/panel.viewport.size.y)*panel.mesh.size.y,0.025)
	var direction: Vector3 = (panel.to_global(local)-scene.camera.global_position).normalized()
	var motion := InputEventMouseMotion.new()
	motion.position = root.size*0.5
	motion.relative = Vector2(wrapf(scene.orbit_yaw-atan2(-direction.x,-direction.z),-PI,PI),scene.orbit_pitch-asin(direction.y))/0.003
	root.push_input(motion,true)
	await process_frame
	await process_frame
	var hit: Dictionary = scene.world_panels.pick(root.size*0.5)
	if hit.get("id","") != panel.panel_id:
		push_error("Panel control is occluded: expected="+panel.panel_id+" hit="+str(hit.get("id","none"))); quit(1); return
	mouse(MOUSE_BUTTON_LEFT,true,root.size*0.5)
	mouse(MOUSE_BUTTON_LEFT,false,root.size*0.5)

func run() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and not scene.current.is_empty(),"Scene not ready"): quit(1); return
	scene.hud.set_edit_mode(true)
	var target: String = scene.selected_id
	for _i in 2: scene.world_panels.open_inspector(target)
	if not await until(func(): return scene.world_panels.panels.size() == 2,"Shared inspectors missing"): quit(1); return
	var panels: Array = scene.world_panels.panels.values()
	for panel in panels: panel.content.tabs.current_tab = 1
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
	# Use the actual FPS ray/input path for removal and creation, not signal-only tests.
	scene.world_panels.close_panel(panels[1].panel_id)
	if not await until(func(): return scene.world_panels.panels.size()==1,"Extra inspector did not close"): quit(1); return
	var panel = panels[0]
	scene.set_view_mode(1)
	scene.orbit_yaw = 0
	scene.orbit_pitch = 0
	var position: Vector3 = scene.player_position + Vector3(0,1.15,-2)
	scene.world_panels.edit(panel.panel_id,"position",[position.x,position.y,position.z])
	scene.world_panels.edit(panel.panel_id,"rotation",[0,0,0,1])
	if not await until(func(): return panel.position.distance_to(position)<0.01 and not a.editor.remove_button.disabled,"Removal controls not ready"): quit(1); return
	await fps_click(scene,panel,a.editor.remove_button)
	if root.find_children("*","Window",true,false).any(func(w): return w.visible and w.exclusive):
		push_error("Script removal opened an exclusive subwindow that blocks world input"); quit(1); return
	if not await until(func(): return a.editor.remove_confirmation.visible,"Inline removal confirmation did not open"): quit(1); return
	if not scene.fps_active or scene.gameplay_keyboard_blocked(): push_error("Removal confirmation blocks FPS movement"); quit(1); return
	var before: Vector3 = scene.player_target_position
	Input.action_press("move_right")
	var moved: bool = await until(func(): return scene.player_target_position.distance_to(before)>0.15,"Removal confirmation froze movement")
	Input.action_release("move_right")
	if not moved: quit(1); return
	await fps_click(scene,panel,a.editor.remove_cancel_button)
	if a.editor.remove_confirmation.visible or scene.objects[target].scripts.is_empty(): push_error("Cancel removed the script or left confirmation open"); quit(1); return
	await fps_click(scene,panel,a.editor.remove_button)
	await fps_click(scene,panel,a.editor.remove_confirm_button)
	if not await until(func(): return scene.objects[target].get("scripts",[]).is_empty() and a.editor.pending_id.is_empty(),"FPS script removal failed"): quit(1); return
	await fps_click(scene,panel,a.editor.graph_button)
	if not await until(func(): return scene.objects[target].get("scripts",[]).size() == 1 and a.editor.pending_id.is_empty() and a.editor.is_graph(),"FPS node graph creation failed"): quit(1); return
	if not a.document.is_empty() or not a.editor.graph.visible:
		push_error("Graph editor incorrectly uses the Lua text draft"); quit(1); return
	a.editor.graph.add_node("on_interact")
	await fps_click(scene,panel,a.editor.apply_button)
	if not await until(func(): return a.editor.pending_id.is_empty() and scene.objects[target].scripts[0].source.contains("on_interact"),"Shared graph Apply failed"): quit(1); return
	scene.release_cursor()
	await process_frame
	var graph_point: Vector2 = a.editor.graph.canvas.get_global_rect().position + Vector2(120,55)
	var graph_local := Vector3((graph_point.x/panel.viewport.size.x-0.5)*panel.mesh.size.x,(0.5-graph_point.y/panel.viewport.size.y)*panel.mesh.size.y,0.025)
	var graph_screen: Vector2 = scene.camera.unproject_position(panel.to_global(graph_local))
	mouse(MOUSE_BUTTON_RIGHT,true,graph_screen)
	mouse(MOUSE_BUTTON_RIGHT,false,graph_screen)
	if not await until(func(): return a.editor.graph.context_panel.visible,"Right-click did not reach the shared graph canvas"): quit(1); return
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			panel.viewport.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").get_base_dir().path_join("graph-context.png"))
	a.editor.graph.context_panel.hide()
	scene.set_view_mode(1)
	# The New entry must open an operable world panel even immediately after removal.
	mouse(MOUSE_BUTTON_MIDDLE,true,root.size*0.5)
	mouse(MOUSE_BUTTON_MIDDLE,false,root.size*0.5)
	if not scene.hud.ring.visible: push_error("Ring blocked after script removal"); quit(1); return
	var new_point: Vector2 = scene.hud.ring.center + Vector2(0,125)
	mouse(MOUSE_BUTTON_LEFT,true,new_point)
	mouse(MOUSE_BUTTON_LEFT,false,new_point)
	if not await until(func(): return scene.world_panels.panels.values().any(func(p): return not p.inspector_content),"New object dialog did not appear after script removal"): quit(1); return
	var creation = scene.world_panels.panels.values().filter(func(p): return not p.inspector_content)[0]
	var creation_id: String = creation.panel_id
	if not await until(func(): return not scene.gameplay_keyboard_blocked() and scene.fps_active,"New object panel blocks FPS"): quit(1); return
	var count: int = scene.objects.size()
	await fps_click(scene,creation,creation.content.create_button)
	if not await until(func(): return not scene.objects.has(creation_id) and scene.objects.size()==count,"Creating an object after script removal failed"): quit(1); return
	print("SCRIPT DIALOGS PASS: inline cancel/remove, FPS movement, New panel and creation")
	print("SHARED SCRIPT SMOKE PASS: concurrent drafts, multiple cursors, Apply and syntax errors")
	quit(0)
