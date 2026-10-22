extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func mouse(button: int, pressed: bool, shift: bool = false) -> void:
	var event := InputEventMouseButton.new()
	event.button_index = button
	event.pressed = pressed
	event.shift_pressed = shift
	event.position = root.size * 0.5
	event.global_position = event.position
	root.push_input(event, true)

func setup_box(scene: Node, position: Vector3) -> bool:
	print("INTERACTION_SETUP " + JSON.stringify({"target":scene.address_of(scene.current),"position":[position.x,position.y,position.z]}))
	for _i in range(150):
		await create_timer(0.03).timeout
		var p: Array = scene.current.world_position
		if Vector3(p[0],p[1],p[2]).distance_to(position) < 0.01: return true
	return false

func run() -> void:
	var scene: Node = load("res://main.tscn").instantiate()
	root.add_child(scene)
	for _i in range(200):
		await create_timer(0.03).timeout
		if scene.editable and scene.player_initialized: break
	if not scene.editable or not scene.player_initialized:
		push_error("Interaction client did not connect"); quit(1); return
	for id in scene.objects:
		if scene.objects[id].visual.kind == "cube":
			scene.select_object(id)
			break
	var p: Vector3 = scene.player_target_position
	if not await setup_box(scene, Vector3(p.x,0.75,p.z-3.0)):
		push_error("Interaction setup failed"); quit(1); return
	scene.set_view_mode(1)
	scene.orbit_yaw = 0.0
	scene.orbit_pitch = 0.0
	await create_timer(0.1).timeout
	mouse(MOUSE_BUTTON_RIGHT,true)
	for _i in range(100):
		await create_timer(0.02).timeout
		if not scene.hand_grab.is_empty(): break
	if scene.hand_grab.is_empty():
		push_error("RMB did not grab: " + scene.feedback.text); quit(1); return
	if not scene.aiming_at_collider or scene.laser_mesh.get_surface_count() == 0:
		push_error("Rust interaction ray was not rendered"); quit(1); return
	var distance: float = scene.hand_grab.distance
	var motion := InputEventMouseMotion.new()
	motion.position = root.size * 0.5
	motion.relative = Vector2(120,0)
	root.push_input(motion,true)
	await create_timer(0.2).timeout
	mouse(MOUSE_BUTTON_WHEEL_UP,true)
	await create_timer(0.15).timeout
	if scene.hand_grab.is_empty() or scene.hand_grab.distance <= distance:
		push_error("Wheel did not change grab distance"); quit(1); return
	var camera_yaw: float = scene.orbit_yaw
	var before_rotation: Array = scene.hand_grab.preview.rotation.duplicate()
	Input.action_press("grab_rotate")
	motion.relative = Vector2(45,25)
	root.push_input(motion,true)
	Input.action_release("grab_rotate")
	await create_timer(0.15).timeout
	var rotation: Array = scene.hand_grab.preview.rotation
	if rotation[0] <= 0.02 or rotation[1] <= before_rotation[1]+0.05 or scene.orbit_yaw != camera_yaw:
		push_error("E+mouse did not rotate object with camera frozen"); quit(1); return
	mouse(MOUSE_BUTTON_WHEEL_UP,true,true)
	await create_timer(0.15).timeout
	if absf(scene.hand_grab.preview.scale[0]-1.1) > 0.001:
		push_error("Shift+wheel did not scale object"); quit(1); return
	mouse(MOUSE_BUTTON_LEFT,true)
	await create_timer(0.15).timeout
	rotation = scene.hand_grab.preview.rotation.duplicate()
	var up := Quaternion(rotation[0],rotation[1],rotation[2],rotation[3])*Vector3.UP
	if up.distance_to(Vector3.UP) > 0.001 or absf(rotation[1]) < 0.01:
		push_error("LMB did not straighten object while preserving heading"); quit(1); return
	Input.action_press("grab_rotate")
	motion.relative = Vector2(40,0)
	root.push_input(motion,true)
	Input.action_release("grab_rotate")
	await create_timer(0.15).timeout
	rotation = scene.hand_grab.preview.rotation.duplicate()
	var held: Array = scene.hand_grab.preview.position.duplicate()
	if held[0] < p.x + 0.4:
		push_error("Held object did not follow FPS view"); quit(1); return
	mouse(MOUSE_BUTTON_RIGHT,false)
	for _i in range(150):
		await create_timer(0.03).timeout
		if not scene.pending and scene.hand_grab.is_empty(): break
	if scene.pending or not scene.hand_grab.is_empty():
		push_error("RMB release did not commit"); quit(1); return
	if scene.current.world_rotation != rotation or absf(scene.current.world_scale[0]-1.1) > 0.001:
		push_error("Rotation/scale did not persist on RMB release"); quit(1); return
	var placed: Array = scene.current.world_position
	if Vector3(placed[0],placed[1],placed[2]).distance_to(Vector3(held[0],held[1],held[2])) > 0.05:
		push_error("Placed position differs from held position"); quit(1); return
	mouse(MOUSE_BUTTON_RIGHT,true)
	await create_timer(0.15).timeout
	motion.relative = Vector2(-90,0)
	root.push_input(motion,true)
	await create_timer(0.1).timeout
	var escape := InputEventKey.new()
	escape.keycode = KEY_ESCAPE
	escape.pressed = true
	root.push_input(escape,true)
	await create_timer(0.15).timeout
	if not scene.hand_grab.is_empty() or scene.current.world_position != placed or scene.fps_active:
		push_error("Escape did not cancel grab and release cursor"); quit(1); return
	p = scene.player_target_position
	if not await setup_box(scene, Vector3(p.x,0.75,p.z-1.5)):
		push_error("Platform setup failed"); quit(1); return
	scene.set_view_mode(1)
	scene.orbit_yaw = 0.0
	scene.orbit_pitch = 0.0
	Input.action_press("move_forward")
	Input.action_press("jump")
	await create_timer(0.5).timeout
	Input.action_release("move_forward")
	Input.action_release("jump")
	await create_timer(0.65).timeout
	if absf(scene.player_target_position.y-1.5) > 0.02 or not scene.grounded:
		push_error("FPS player did not land on box: " + str(scene.player_target_position)); quit(1); return
	var before: Vector3 = scene.player_target_position
	var box: Array = scene.current.world_position
	if not await setup_box(scene,Vector3(box[0]+0.6,box[1],box[2])):
		push_error("Moving platform setup failed"); quit(1); return
	await create_timer(0.2).timeout
	if absf(scene.player_target_position.x-before.x-0.6) > 0.02 or absf(scene.player_target_position.y-1.5) > 0.02:
		push_error("Standing player did not follow moved box"); quit(1); return
	print("INTERACTION SMOKE PASS")
	quit(0)
