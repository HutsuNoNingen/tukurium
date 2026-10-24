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
	scene.hud.set_edit_mode(true)
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
	# Carry a field into the collaborative code editor, choose Drive, and save both code and binding.
	var dragged_source := "return {\n    name = \"DraggedField\",\n    fields = {},\n    drive = function(ctx)\n        \n    end\n}\n"
	reply.clear()
	scene.world_panels.command({"kind":"script","command":{"target":scene.address_of(scene.objects[target]),"action":"attach","source":dragged_source}},func(result): reply.merge(result))
	if not await until(func(): return reply.has("ok") and scene.objects[target].scripts.size()==2,"Dragged script missing"): quit(1); return
	if not reply.ok: push_error(str(reply.get("message","Could not attach dragged script"))); quit(1); return
	var dragged_script: String = str(scene.objects[target].scripts.filter(func(item): return item.schema.name=="DraggedField")[0].id)
	target_view.open_script(dragged_script)
	if not await until(func(): return target_view.script_view.editor.selected==dragged_script and target_view.script_view.editor.code.text==dragged_source,"Dragged editor did not open"): quit(1); return
	var editor = target_view.script_view.editor
	editor.code.set_caret_line(4)
	editor.code.set_caret_column(8)
	event.button_index = MOUSE_BUTTON_RIGHT
	event.ctrl_pressed = true
	chip._gui_input(event)
	if scene.world_panels.carried_reference.is_empty(): push_error("Field was not carried into code editor"); quit(1); return
	event.button_index = MOUSE_BUTTON_LEFT
	event.ctrl_pressed = false
	editor.code._gui_input(event)
	if not editor.drop_choices.visible: push_error("Code editor did not offer field operations"); quit(1); return
	if not scene.world_panels.carried_reference.is_empty(): push_error("Code editor did not consume field reference"); quit(1); return
	for choice in editor.drop_choices.get_children():
		if choice is Button and choice.text=="Drive": choice.pressed.emit()
	if not editor.code.text.contains("ctx:drive_field(ctx.self.DraggedField.ref_position"): push_error("Drive source was not inserted"); quit(1); return
	if not editor.code.text.contains("ref_position = { type = \"field_ref\""): push_error("Field binding declaration was not inserted"); quit(1); return
	await create_timer(0.35).timeout
	if not editor.queued_parameters.has("ref_position"): push_error("Draft reference flushed before Apply"); quit(1); return
	editor.apply_button.pressed.emit()
	if not await until(func(): return scene.objects[target].scripts.any(func(item): return item.schema.name=="DraggedField" and item.parameters.get("ref_position",{}).get("kind")=="field"),"Dropped field binding was not saved"): push_error(editor.feedback.text); quit(1); return
	if not await until(func(): return scene.world_panels.field_driver(source,"position").get("schema",{}).get("name")=="DraggedField","Dropped Drive was not evaluated"):
		push_error("Saved source="+str(scene.objects[target].scripts.filter(func(item): return item.schema.name=="DraggedField")[0].source)+" statuses="+str(scene.local_script_statuses))
		quit(1); return
	# A WorldText Drive renders locally while its shared text stays unchanged.
	reply.clear()
	scene.world_panels.command({"kind":"structure","command":{"action":"create_slot","parent":scene.address_of(scene.objects[target]),"position":[0.0,0.0,0.8],"position_space":"parent","components":[{"type_id":"WorldText","values":{"world_text":"Text","world_text_color":[1.0,1.0,1.0,1.0]}}]}},func(result): reply.merge(result))
	if not await until(func(): return reply.has("selected"),"WorldText Slot missing"): quit(1); return
	var text_id: String = str(reply.selected.instance_id)
	var clock_source := "return {\n    name = \"TextClock\",\n    fields = {},\n    drive = function(ctx)\n        \n    end\n}\n"
	reply.clear()
	scene.world_panels.command({"kind":"script","command":{"target":scene.address_of(scene.objects[target]),"action":"attach","source":clock_source}},func(result): reply.merge(result))
	if not await until(func(): return reply.has("ok") and scene.objects[target].scripts.size()==3,"TextClock script missing"): quit(1); return
	var clock_id: String = str(scene.objects[target].scripts.filter(func(item): return item.schema.name=="TextClock")[0].id)
	target_view.open_script(clock_id)
	if not await until(func(): return target_view.script_view.editor.selected==clock_id and target_view.script_view.editor.code.text==clock_source,"TextClock editor did not open"): quit(1); return
	editor = target_view.script_view.editor
	editor.code.set_caret_line(4)
	editor.code.set_caret_column(8)
	scene.world_panels.carried_reference = {"kind":"kokoverse_reference","reference":{"kind":"field","target":scene.field_address(text_id,"world_text"),"field_id":"world_text"}}
	editor.code._gui_input(event)
	if not editor.drop_choices.visible: push_error("WorldText drop choices missing"); quit(1); return
	for choice in editor.drop_choices.get_children():
		if choice is Button and choice.text=="Drive": choice.pressed.emit()
	if not editor.code.text.contains("string.format(\"%.1f\", ctx.time)"): push_error("WorldText clock source was not inserted"); quit(1); return
	editor.apply_button.pressed.emit()
	if not await until(func(): return scene.objects[target].scripts.any(func(item): return item.schema.name=="TextClock" and item.parameters.get("ref_world_text",{}).get("kind")=="field"),"WorldText binding was not saved"): quit(1); return
	if not await until(func(): return scene.driven_objects.get(text_id,{}).has("world_text") and scene.render_nodes.has(text_id) and scene.render_nodes[text_id].has_node("WorldText") and scene.render_nodes[text_id].get_node("WorldText").text != "Text","WorldText Drive was not rendered"):
		push_error("Text statuses="+str(scene.local_script_statuses)+" feedback="+editor.feedback.text)
		quit(1); return
	if scene.objects[text_id].world_text.text != "Text": push_error("Drive changed shared WorldText"); quit(1); return
	# Write uses the same dropped reference but commits its value through the host.
	var write_source := "return {\n    name = \"TextWriter\",\n    fields = {},\n    on_interact = function(ctx)\n        \n    end\n}\n"
	reply.clear()
	scene.world_panels.command({"kind":"script","command":{"target":scene.address_of(scene.objects[target]),"action":"attach","source":write_source}},func(result): reply.merge(result))
	if not await until(func(): return reply.has("ok") and scene.objects[target].scripts.size()==4,"TextWriter script missing"): quit(1); return
	var writer_id: String = str(scene.objects[target].scripts.filter(func(item): return item.schema.name=="TextWriter")[0].id)
	target_view.open_script(writer_id)
	if not await until(func(): return target_view.script_view.editor.selected==writer_id and target_view.script_view.editor.code.text==write_source,"TextWriter editor did not open"): quit(1); return
	editor = target_view.script_view.editor
	editor.code.set_caret_line(4)
	editor.code.set_caret_column(8)
	scene.world_panels.carried_reference = {"kind":"kokoverse_reference","reference":{"kind":"field","target":scene.field_address(text_id,"world_text"),"field_id":"world_text"}}
	editor.code._gui_input(event)
	if not editor.drop_choices.visible: push_error("WorldText Write choice missing"); quit(1); return
	for choice in editor.drop_choices.get_children():
		if choice is Button and choice.text=="Write": choice.pressed.emit()
	if not editor.code.text.contains("ctx:write_field(ctx.self.TextWriter.ref_world_text"): push_error("Write source was not inserted"); quit(1); return
	editor.code.text = editor.code.text.replace("\"Text\")", "\"Started\")")
	editor.apply_button.pressed.emit()
	if not await until(func(): return scene.objects[target].scripts.any(func(item): return item.schema.name=="TextWriter" and item.parameters.get("ref_world_text",{}).get("kind")=="field"),"WorldText Write binding was not saved"): quit(1); return
	reply.clear()
	scene.world_panels.command({"kind":"script","command":{"target":scene.address_of(scene.objects[target]),"action":"interact"}},func(result): reply.merge(result))
	if not await until(func(): return reply.has("ok") and scene.objects[text_id].world_text.text=="Started","WorldText Write did not commit"):
		push_error("Write status="+str(reply)+" scripts="+str(scene.script_statuses))
		quit(1); return
	var native_drop := {"kind":"kokoverse_reference","reference":{"kind":"field","target":scene.field_address(text_id,"world_text"),"field_id":"world_text"}}
	if not editor.code._can_drop_data(Vector2(60,60),native_drop): push_error("Native field drag was rejected"); quit(1); return
	editor.code._drop_data(Vector2(60,60),native_drop)
	if not editor.drop_choices.visible: push_error("Native field drag did not show operations"); quit(1); return
	for choice in editor.drop_choices.get_children():
		if choice is Button and choice.text=="Cancel": choice.pressed.emit()
	target_view.choose_target(text_id)
	if not await until(func(): return target_view.target_id==text_id and target_view.drive_labels.has("world_text"),"WorldText inspector did not open"): quit(1); return
	if not await until(func(): return target_view.drive_labels.world_text.visible,"WorldText Drive was not shown in inspector"): quit(1); return
	target_view.open_driver("world_text")
	if not await until(func(): return target_view.target_id==target and target_view.script_view.editor.selected==clock_id,"External Drive source did not open"): quit(1); return
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
