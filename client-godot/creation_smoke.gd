extends SceneTree
const PRIMITIVES = preload("res://primitive_mesh.gd")

func _initialize() -> void: call_deferred("run")

func until(test: Callable, message: String) -> bool:
	for _i in 800:
		await create_timer(0.025).timeout
		if test.call(): return true
	push_error(message)
	return false

func click(at: Vector2, button: int = MOUSE_BUTTON_LEFT) -> void:
	var motion := InputEventMouseMotion.new()
	motion.position=at
	root.push_input(motion,true)
	var event := InputEventMouseButton.new()
	event.button_index=button; event.position=at; event.global_position=at; event.pressed=true
	root.push_input(event,true)
	event=event.duplicate(); event.pressed=false
	root.push_input(event,true)

func mouse(button: int, pressed: bool, shift: bool = false) -> void:
	var event := InputEventMouseButton.new()
	event.button_index=button; event.pressed=pressed; event.shift_pressed=shift
	event.position=root.size*0.5
	root.push_input(event,true)

func click_control(scene, surface, control: Control) -> void:
	var point := control.get_global_rect().get_center()
	var local := Vector3((point.x/surface.viewport.size.x-0.5)*surface.mesh.size.x,(0.5-point.y/surface.viewport.size.y)*surface.mesh.size.y,0.025)
	click(scene.camera.unproject_position(surface.to_global(local)))

func run() -> void:
	var scene=load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and scene.player_initialized,"Scene not ready"): quit(1); return
	click(root.size*0.5,MOUSE_BUTTON_MIDDLE)
	click(scene.hud.ring.center+Vector2(0,125))
	if not await until(func(): return scene.world_panels.panels.size()==1,"Ring did not open shared creation panel"): quit(1); return
	var surface=scene.world_panels.panels.values()[0]
	var panel=surface.content
	var id: String=surface.panel_id
	await create_timer(0.2).timeout
	if not surface is Node3D or surface.inspector_content or scene.objects[id].get("creation")==null or not scene.first_person or scene.view_mode.selected!=1: push_error("Creation is not a shared world surface"); quit(1); return
	if scene.gameplay_keyboard_blocked(): push_error("Creation panel blocks gameplay"); quit(1); return
	scene.select_object(id)
	mouse(MOUSE_BUTTON_RIGHT,true)
	if not await until(func(): return not scene.hand_grab.is_empty(),"Could not grab creation panel"): quit(1); return
	if scene.hand_grab.preview.target.instance_id!=id: push_error("Grab hit another object"); quit(1); return
	Input.action_press("grab_rotate")
	var motion := InputEventMouseMotion.new()
	motion.position=root.size*0.5; motion.relative=Vector2(15,10)
	root.push_input(motion,true)
	Input.action_release("grab_rotate")
	mouse(MOUSE_BUTTON_WHEEL_UP,true,true)
	await create_timer(0.2).timeout
	mouse(MOUSE_BUTTON_RIGHT,false)
	if not await until(func(): return scene.hand_grab.is_empty() and not scene.pending and scene.objects[id].world_scale[0]>1.05,"Creation transform did not commit"): quit(1); return
	if absf(scene.objects[id].world_rotation[1])<0.01: push_error("Creation panel did not rotate"); quit(1); return
	await create_timer(0.2).timeout
	var before: Vector3=scene.player_target_position
	var fixed_position: Array=scene.objects[id].world_position.duplicate()
	Input.action_press("move_right")
	var moved: bool=await until(func(): return Vector2(scene.player_target_position.x,scene.player_target_position.z).distance_to(Vector2(before.x,before.z))>0.15,"Cannot move while creation panel is open")
	Input.action_release("move_right")
	if not moved: quit(1); return
	if scene.objects[id].world_position!=fixed_position: push_error("Panel followed player without being held"); quit(1); return
	if not await until(func(): return scene.grounded,"Player did not settle before jump"): quit(1); return
	var floor_y: float=scene.player_target_position.y
	Input.action_press("jump")
	var jumped: bool=await until(func(): return scene.player_target_position.y>floor_y+0.1,"Cannot jump while creation panel is open")
	Input.action_release("jump")
	if not jumped: quit(1); return
	if not await until(func(): return scene.grounded,"Player did not land after jump"): quit(1); return
	scene.release_cursor()
	var initial_count: int=scene.objects.size()
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").replace(".png","-palette.png"))
	for index in PRIMITIVES.SHAPES.size():
		click_control(scene,surface,panel.cards[index])
		if not await until(func(): return scene.objects[id].creation.shape==PRIMITIVES.SHAPES[index] and panel.selected==index,"3D selection was not shared"): quit(1); return
		click_control(scene,surface,panel.create_button)
		if not await until(func(): return not panel.busy and scene.objects.size()==initial_count+index+1 and scene.current.visual.get("shape")==PRIMITIVES.SHAPES[index],"Primitive not created/selected: "+PRIMITIVES.NAMES[index]): quit(1); return
		var object: Dictionary=scene.current
		if object.visual.get("shape")!=PRIMITIVES.SHAPES[index] or scene.render_nodes[scene.selected_id].mesh.get_class()!=PRIMITIVES.create(PRIMITIVES.SHAPES[index]).get_class():
			push_error("Wrong procedural mesh: expected "+PRIMITIVES.SHAPES[index]+", got "+str(object.visual)+", pending="+str(scene.pending)); quit(1); return
		if not scene.first_person or scene.view_mode.selected!=1: push_error("Creation switched away from FPS"); quit(1); return
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot="))
	var escape := InputEventKey.new()
	escape.keycode=KEY_ESCAPE; escape.pressed=true
	root.push_input(escape,true)
	if not scene.world_panels.panels.has(id) or Input.mouse_mode!=Input.MOUSE_MODE_VISIBLE: push_error("Esc deleted world panel or captured cursor"); quit(1); return
	print("CREATION SMOKE PASS: shared surface, move while open, grab/rotate/scale, shared selection, eight meshes, no camera switch")
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
