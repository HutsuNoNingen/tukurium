extends SceneTree

func _initialize() -> void: call_deferred("run")
func until(test: Callable, message: String) -> bool:
	for _i in 600:
		await create_timer(0.025).timeout
		if test.call(): return true
	push_error(message)
	return false

func run() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and not scene.current.is_empty(),"Not ready"): quit(1); return
	var source: String = scene.selected_id
	scene.world_panels.open_inspector(source)
	if not await until(func(): return scene.world_panels.panels.size()==1,"Source inspector missing"): quit(1); return
	var source_view = scene.world_panels.panels.values()[0].content
	var reply := {}
	scene.world_panels.command({"kind":"structure","command":{"action":"create"}},func(result): reply.merge(result))
	if not await until(func(): return reply.has("selected"),"Target missing"): quit(1); return
	var target: String = reply.selected.instance_id
	var code := "return {name='CopyPosition',fields={source={type='field_ref',label='Source position'}},drive=function(ctx) local value=ctx:read(ctx.self.CopyPosition.source) if value then ctx:drive(ctx.self,'position',value) end end}"
	reply.clear()
	scene.world_panels.command({"kind":"script","command":{"target":scene.address_of(scene.objects[target]),"action":"attach","source":code}},func(result): reply.merge(result))
	if not await until(func(): return reply.has("ok"),"Attach missing"): quit(1); return
	if not reply.ok: push_error(str(reply.get("message","Unknown script error"))); quit(1); return
	scene.world_panels.open_inspector(target,true)
	if not await until(func(): return scene.world_panels.windows.size()==1,"Native target inspector missing"): quit(1); return
	var window: Window = scene.world_panels.windows.values()[0]
	var target_view = window.get_meta("inspector")
	target_view.open_script(str(scene.objects[target].scripts[0].id))
	if not await until(func(): return target_view.script_view.editor.parameter_controls.has("source") and not target_view.script_view.editor.parameter_controls.source.disabled,"Reference parameter missing"): quit(1); return
	var destination = target_view.script_view.editor.parameter_controls.source
	var chip = source_view.reference_chips.position
	var event := InputEventMouseButton.new()
	event.pressed = true
	event.button_index = MOUSE_BUTTON_RIGHT
	chip._gui_input(event)
	if not scene.world_panels.carried_reference.is_empty(): push_error("Plain RMB took a reference"); quit(1); return
	event.ctrl_pressed = true
	chip._gui_input(event)
	if scene.world_panels.carried_reference.is_empty(): push_error("Reference not picked up"); quit(1); return
	if not destination._can_drop_data(Vector2.ZERO,scene.world_panels.carried_reference): push_error("Field rejected"); quit(1); return
	event.button_index = MOUSE_BUTTON_LEFT
	event.ctrl_pressed = false
	destination._gui_input(event)
	if not await until(func(): return scene.objects[target].scripts[0].parameters.source!=null,"Reference not shared"): quit(1); return
	if not await until(func(): return scene.world_panels.field_driver(target,"position").get("schema",{}).get("name")=="CopyPosition","Reference not evaluated"): quit(1); return
	if not scene.world_panels.carried_reference.is_empty(): push_error("Reference remained held"); quit(1); return
	destination = target_view.script_view.editor.parameter_controls.source
	if destination._can_drop_data(Vector2.ZERO,{"kind":"kokoverse_reference","reference":{"kind":"slot","target":scene.objects[source].address}}): push_error("Wrong reference kind accepted"); quit(1); return
	# Independent native and spatial views must occupy separate editor sessions.
	scene.world_panels.open_inspector(target)
	if not await until(func(): return scene.world_panels.panels.size()==2,"Explicit target world panel missing"): quit(1); return
	var world_target = scene.world_panels.panels.values()[1].content
	if world_target.input_client_id==target_view.input_client_id: push_error("Native and spatial editors share an input owner"); quit(1); return
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			window.get_texture().get_image().save_png(argument.trim_prefix("--screenshot="))
	print("REFERENCE SMOKE PASS")
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
