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
	var parent: String = scene.selected_id
	scene.world_panels.open_inspector(parent,true)
	if not await until(func(): return scene.world_panels.windows.size()==1,"No window"): quit(1); return
	var view = scene.world_panels.windows.values()[0].get_meta("inspector")
	view.hierarchy.child_button.pressed.emit()
	if not await until(func(): return view.target_id!=parent and not view.target_id.is_empty(),"Child not selected"): quit(1); return
	var child: String = view.target_id
	if scene.objects[child].parent.instance_id != parent: push_error("Child parent missing"); quit(1); return
	scene.world_panels.edit(child,"position",[2,0,0])
	if not await until(func(): return scene.world_panels.field_data(child,"position").value[0]==2,"Local position not set"): quit(1); return
	await create_timer(0.1).timeout
	var expected: Dictionary = scene.world_panels.evaluated_transforms.get(child,{})
	if expected.is_empty(): push_error("No inherited transform"); quit(1); return
	if scene.render_nodes[child].position.distance_to(Vector3(expected.position[0],expected.position[1],expected.position[2]))>0.02: push_error("Renderer disagrees with Rust"); quit(1); return
	# Send a real preview through the engine while the parent also has a local Drive.
	var revision: String = scene.world_panels.field_data(parent,"position").revision
	scene.queue_message({"kind":"drag_preview","preview":{"target":scene.address_of(scene.objects[parent]),"expected_revision":revision,"position":[5,2,1]}})
	if not await until(func(): return scene.remote_previews.has(parent),"Parent preview not received"): quit(1); return
	for _frame in 12:
		await process_frame
		var parent_node: Node3D = scene.render_nodes[parent]
		var expected_child: Vector3 = parent_node.position + parent_node.quaternion * (parent_node.scale * Vector3(2,0,0))
		if scene.render_nodes[child].position.distance_to(expected_child)>0.001: push_error("Child does not follow displayed parent during preview"); quit(1); return
	scene.queue_message({"kind":"drag_preview","preview":null})
	if not await until(func(): return not scene.remote_previews.has(parent),"Preview not released"): quit(1); return
	view.hierarchy.set_root(scene.objects[parent].address)
	if not await until(func(): return view.panel_state.inspector.tree.root!=null,"Local hierarchy root did not change"): quit(1); return
	if not scene.world_panels.panels.is_empty(): push_error("Local hierarchy created a world panel"); quit(1); return
	var tree_root: TreeItem = view.hierarchy.tree.get_root()
	if tree_root.get_metadata(0)!=parent: push_error("Hierarchy root is wrong"); quit(1); return
	# Drive is local; detaching must preserve the authoritative base pose.
	var before: Array = scene.objects[child].world_position.duplicate()
	view.hierarchy.move_object(child,null)
	if not await until(func(): return scene.objects[child].parent==null,"Reparent did not commit"): quit(1); return
	if not await until(func(): return Vector3(before[0],before[1],before[2]).distance_to(scene.render_nodes[child].position)<0.02,"Reparent moved authoritative pose"): quit(1); return
	view.hierarchy.move_object(child,parent)
	if not await until(func(): return scene.objects[child].parent!=null,"Parent restore failed"): quit(1); return
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			view.get_viewport().get_texture().get_image().save_png(argument.trim_prefix("--screenshot="))
	print("HIERARCHY SMOKE PASS")
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
