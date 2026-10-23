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

func click_control(scene, panel, control: Control) -> void:
	var point := control.get_global_rect().get_center()
	var local := Vector3((point.x/panel.PIXELS.x-0.5)*panel.METERS.x,(0.5-point.y/panel.PIXELS.y)*panel.METERS.y,0)
	click(scene.camera.unproject_position(panel.to_global(local)))

func run() -> void:
	var scene=load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and scene.player_initialized,"Scene not ready"): quit(1); return
	click(root.size*0.5,MOUSE_BUTTON_MIDDLE)
	click(scene.hud.ring.center+Vector2(0,125))
	if not await until(func(): return is_instance_valid(scene.hud.creation_panel),"Ring did not open creation panel"): quit(1); return
	var panel=scene.hud.creation_panel
	await create_timer(0.2).timeout
	if not panel is Node3D or not scene.first_person or scene.view_mode.selected!=1: push_error("Creation menu changed camera or is not a 3D panel"); quit(1); return
	var initial_count: int=scene.objects.size()
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").replace(".png","-palette.png"))
	for index in PRIMITIVES.SHAPES.size():
		click_control(scene,panel,panel.cards[index])
		if panel.selected!=index: push_error("3D pointer did not select shape"); quit(1); return
		click_control(scene,panel,panel.create_button)
		if not await until(func(): return not panel.busy and scene.objects.size()==initial_count+index+1 and scene.current.visual.get("shape")==PRIMITIVES.SHAPES[index],"Primitive not created/selected: "+PRIMITIVES.NAMES[index]): quit(1); return
		var object: Dictionary=scene.current
		if object.visual.get("shape")!=PRIMITIVES.SHAPES[index] or scene.render_nodes[scene.selected_id].mesh.get_class()!=PRIMITIVES.create(PRIMITIVES.SHAPES[index]).get_class():
			push_error("Wrong procedural mesh: expected "+PRIMITIVES.SHAPES[index]+", got "+str(object.visual)+", pending="+str(scene.pending)); quit(1); return
		if not scene.first_person or scene.view_mode.selected!=1: push_error("Creation switched away from FPS"); quit(1); return
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot="))
	print("CREATION SMOKE PASS: 3D ray input, eight procedural meshes, no camera switch")
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
