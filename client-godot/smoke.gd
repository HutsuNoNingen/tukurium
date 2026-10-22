extends SceneTree

func verify_remote_preview(scene: Node) -> bool:
	var before: Vector3 = scene.cube.position
	var target := before + Vector3(-0.75, 0.5, 0)
	var field_before: Array = scene.fields.position.value.duplicate()
	print("REMOTE_PREVIEW " + JSON.stringify({"target":scene.address_of(scene.current),"expected_revision":scene.fields.position.revision,"position":[target.x,target.y,target.z]}))
	var moved := false
	for _i in range(100):
		await create_timer(0.03).timeout
		if scene.cube.position.distance_to(target) < 0.03:
			moved = true
			break
	if not moved or scene.fields.position.value != field_before:
		push_error("Remote live preview did not render without changing canonical fields")
		return false
	for _i in range(100):
		await create_timer(0.03).timeout
		if scene.remote_previews.is_empty(): break
	if not scene.remote_previews.is_empty() or scene.cube.position.distance_to(before) > 0.03:
		push_error("Remote preview cancellation did not restore canonical position")
		return false
	return true

func _initialize() -> void:
	call_deferred("run")

func wait_field(scene: Node, id: String, expected: Variant) -> bool:
	for _i in range(120):
		await create_timer(0.025).timeout
		if not scene.pending and scene.inspector_edits.is_empty() and scene.fields[id].value == expected: return true
	return false

func run() -> void:
	var scene: Node = load("res://main.tscn").instantiate()
	root.add_child(scene)
	for _i in range(200):
		await create_timer(0.05).timeout
		if scene.editable: break
	if not scene.editable:
		push_error("Guest never received authoritative snapshot")
		quit(1)
		return
	for _i in range(200):
		await create_timer(0.05).timeout
		if scene.avatars.size() >= 2: break
	if scene.avatars.size() < 2:
		push_error("Remote avatar did not appear")
		quit(1)
		return
	if scene.avatars[scene.self_id].find_children("*", "MeshInstance3D", true, false).size() != 14:
		push_error("Avatar geometry did not load")
		quit(1)
		return
	scene.set_view_mode(1)
	var player_start: Vector3 = scene.player_target_position
	scene.orbit_yaw = 0.0
	scene.look = true
	Input.action_press("move_right")
	await create_timer(0.2).timeout
	Input.action_release("move_right")
	scene.look = false
	scene.send_player_input()
	await create_timer(0.15).timeout
	if scene.player_target_position.x < player_start.x + 0.4 or absf(scene.player_target_position.z - player_start.z) > 0.02:
		push_error("Godot movement input did not move the Rust character")
		quit(1)
		return
	print("PLAYER_MOVED " + JSON.stringify([scene.player_target_position.x,scene.player_target_position.y,scene.player_target_position.z]))
	if scene.avatars[scene.self_id].visible or scene.camera.position.distance_to(scene.player_position + Vector3(0, 1.15, 0)) > 0.02:
		push_error("First person camera failed")
		quit(1)
		return
	scene.try_jump()
	await create_timer(0.15).timeout
	if scene.player_position.y <= 0.2:
		push_error("Jump did not leave the ground")
		quit(1)
		return
	await create_timer(1.0).timeout
	if not scene.grounded or absf(scene.player_position.y) > 0.01:
		push_error("Jump did not land")
		quit(1)
		return
	scene.set_view_mode(0)
	var toggle := InputEventKey.new()
	toggle.physical_keycode = KEY_I
	toggle.pressed = true
	root.push_input(toggle,true)
	if scene.inspector_shell.visible:
		push_error("I did not hide inspector"); quit(1); return
	scene.inspector_toggle.button_pressed = true
	if not scene.inspector_shell.visible:
		push_error("Inspector button did not restore inspector"); quit(1); return
	scene.name_input.text = "Typing in progress"
	scene.name_input.text_changed.emit(scene.name_input.text)
	var edit_key: String = scene.inspector_key(scene.current.address,"name")
	scene.inspector_edits[edit_key].due = 0
	scene.flush_inspector_edits()
	scene.name_input.text = "Godot integration"
	scene.name_input.text_changed.emit(scene.name_input.text)
	for _i in range(200):
		await create_timer(0.05).timeout
		if not scene.pending and scene.inspector_edits.is_empty() and scene.fields.name.value == "Godot integration": break
	if scene.pending or scene.fields.name.value != "Godot integration":
		print("pending=", scene.pending, " status=", scene.status_label.text, " feedback=", scene.feedback.text, " name=", scene.fields.name.value)
		push_error("Godot form edit failed")
		quit(1)
		return
	scene.color_input.color = Color(0.4, 0.2, 0.8, 1.0)
	scene.color_input.color_changed.emit(scene.color_input.color)
	for _i in range(200):
		await create_timer(0.05).timeout
		if not scene.pending and scene.material.albedo_color.is_equal_approx(Color(0.4, 0.2, 0.8, 1.0)): break
	if scene.pending or not scene.material.albedo_color.is_equal_approx(Color(0.4, 0.2, 0.8, 1.0)):
		push_error("Godot color renderer update failed")
		quit(1)
		return
	for i in range(3): scene.position_inputs[i].get_line_edit().text = str([3.5, 1.25, -2.0][i])
	scene.position_inputs[2].get_line_edit().text_changed.emit(scene.position_inputs[2].get_line_edit().text)
	for _i in range(200):
		await create_timer(0.05).timeout
		if not scene.pending and scene.cube.position.is_equal_approx(Vector3(3.5,1.25,-2)): break
	if scene.pending or not scene.cube.position.is_equal_approx(Vector3(3.5, 1.25, -2)):
		print("position=", scene.cube.position, " fields=", scene.fields.position, " pending=", scene.pending, " feedback=", scene.feedback.text)
		push_error("Godot position renderer update failed")
		quit(1)
		return
	scene.scale_inputs[0].value = 1.25
	if not await wait_field(scene,"scale",[1.25,1.0,1.0]):
		push_error("Scale spinner did not auto-sync"); quit(1); return
	scene.scale_inputs[0].value = 1.0
	if not await wait_field(scene,"scale",[1.0,1.0,1.0]):
		push_error("Scale spinner restore failed"); quit(1); return
	scene.rotation_inputs[1].value = 30
	for _i in range(120):
		await create_timer(0.025).timeout
		if not scene.pending and absf(scene.current.world_rotation[1]-sin(PI/12)) < 0.001: break
	if absf(scene.current.world_rotation[1]-sin(PI/12)) > 0.001:
		push_error("Rotation spinner did not auto-sync"); quit(1); return
	scene.rotation_inputs[1].value = 0
	if not await wait_field(scene,"rotation",[0.0,0.0,0.0,1.0]):
		push_error("Rotation spinner restore failed"); quit(1); return
	var shape: Dictionary = scene.current.collider.duplicate(true)
	scene.collider_solid.button_pressed = false
	var non_solid := shape.duplicate(true)
	non_solid.solid = false
	if not await wait_field(scene,"collider",non_solid):
		push_error("Collider checkbox did not auto-sync"); quit(1); return
	scene.collider_solid.button_pressed = true
	if not await wait_field(scene,"collider",shape):
		push_error("Collider checkbox restore failed"); quit(1); return
	scene.focus_selected()
	if not await verify_remote_preview(scene):
		quit(1)
		return
	for axis in range(3):
		await create_timer(0.05).timeout
		var start: Vector3 = scene.cube.position
		var direction: Vector3 = scene.gizmo.AXES[axis]
		var handle: Vector3 = start + direction * scene.gizmo.unit_scale * 1.1
		var press := InputEventMouseButton.new()
		press.button_index = MOUSE_BUTTON_LEFT
		press.pressed = true
		press.position = scene.camera.unproject_position(handle)
		press.global_position = press.position
		root.push_input(press, true)
		await create_timer(0.03).timeout
		if not scene.gizmo.dragging:
			print("hovered=", root.gui_get_hovered_control(), " selected=", scene.selected_id)
			push_error("Could not pick gizmo axis %d" % axis)
			quit(1)
			return
		var motion := InputEventMouseMotion.new()
		motion.position = scene.camera.unproject_position(handle + direction * 0.5)
		motion.global_position = motion.position
		root.push_input(motion, true)
		await create_timer(0.03).timeout
		if scene.cube.position.distance_to(start + direction * 0.5) > 0.02:
			push_error("Gizmo preview moved off its axis")
			quit(1)
			return
		var cached: Array = scene.fields.position.value
		if Vector3(cached[0], cached[1], cached[2]).distance_to(start) > 0.02:
			push_error("Gizmo mutated committed fields during preview")
			quit(1)
			return
		print("LOCAL_PREVIEW " + JSON.stringify({"target":scene.address_of(scene.current),"position":[scene.cube.position.x,scene.cube.position.y,scene.cube.position.z]}))
		await create_timer(0.5).timeout
		var release := InputEventMouseButton.new()
		release.button_index = MOUSE_BUTTON_LEFT
		release.position = motion.position
		release.global_position = release.position
		release.pressed = false
		root.push_input(release, true)
		for _i in range(200):
			await create_timer(0.05).timeout
			if not scene.pending: break
		if scene.pending or scene.cube.position.distance_to(start + direction * 0.5) > 0.02:
			push_error("Gizmo commit failed")
			quit(1)
			return
	# Escape discards a local preview without sending a new command.
	await create_timer(0.05).timeout
	var before_cancel: Vector3 = scene.cube.position
	var point: Vector3 = before_cancel + Vector3.RIGHT * scene.gizmo.unit_scale * 1.1
	if not scene.gizmo.begin_drag(scene.camera.unproject_position(point)):
		push_error("Gizmo cancellation setup failed")
		quit(1)
		return
	scene.gizmo.update_drag(scene.camera.unproject_position(point + Vector3.RIGHT))
	var escape := InputEventKey.new()
	escape.keycode = KEY_ESCAPE
	escape.pressed = true
	root.push_input(escape, true)
	await create_timer(0.05).timeout
	if scene.gizmo.dragging or scene.pending or scene.cube.position.distance_to(before_cancel) > 0.02:
		push_error("Escape failed to restore gizmo preview")
		quit(1)
		return
	# The integration runner edits the same position from the host while we drag.
	await create_timer(0.05).timeout
	point = scene.cube.position + Vector3.RIGHT * scene.gizmo.unit_scale * 1.1
	if not scene.gizmo.begin_drag(scene.camera.unproject_position(point)):
		push_error("Conflict drag setup failed")
		quit(1)
		return
	scene.gizmo.update_drag(scene.camera.unproject_position(point + Vector3.RIGHT))
	var remote := before_cancel + Vector3(0, 0.5, 0)
	print("GIZMO_CONFLICT " + JSON.stringify({"target":scene.address_of(scene.current),"revision":scene.fields.position.revision,"value":[remote.x,remote.y,remote.z]}))
	for _i in range(200):
		await create_timer(0.05).timeout
		if not scene.gizmo.dragging: break
	if scene.gizmo.dragging or scene.pending or scene.cube.position.distance_to(remote) > 0.02:
		push_error("Remote edit did not cancel drag and restore authoritative position")
		quit(1)
		return
	var picture := Image.create(32, 16, false, Image.FORMAT_RGBA8)
	picture.fill(Color(0.2, 0.8, 0.5, 1))
	var image_path := "user://integration-image.png"
	picture.save_png(image_path)
	scene.import_image(ProjectSettings.globalize_path(image_path))
	for _i in range(200):
		await create_timer(0.05).timeout
		if not scene.pending: break
	if scene.pending or not scene.cube.mesh is QuadMesh or scene.material.albedo_texture == null:
		push_error("Image panel upload or rendering failed")
		quit(1)
		return
	if not await verify_remote_preview(scene):
		quit(1)
		return
	var initial_count: int = scene.objects.size()
	scene.create_button.pressed.emit()
	for _i in range(200):
		await create_timer(0.05).timeout
		if not scene.pending: break
	if scene.objects.size() != initial_count + 1:
		push_error("New cube button failed")
		quit(1)
		return
	var created: String = scene.selected_id
	scene.duplicate_button.pressed.emit()
	for _i in range(200):
		await create_timer(0.05).timeout
		if not scene.pending: break
	if scene.objects.size() != initial_count + 2 or scene.selected_id == created:
		push_error("Duplicate selection failed")
		quit(1)
		return
	var deleted: String = scene.selected_id
	scene.delete_button.pressed.emit()
	var dialogs: Array[Node] = scene.find_children("*", "ConfirmationDialog", true, false)
	if dialogs.is_empty():
		push_error("Delete confirmation missing")
		quit(1)
		return
	dialogs[0].confirmed.emit()
	for _i in range(200):
		await create_timer(0.05).timeout
		if not scene.pending: break
	if scene.objects.has(deleted) or scene.render_nodes.has(deleted) or not scene.objects.has(created):
		push_error("Delete left stale rendering or removed source")
		quit(1)
		return
	print("GODOT SMOKE PASS")
	quit(0)
