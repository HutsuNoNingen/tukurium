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

func aim_control(scene, surface, control: Control) -> Vector2:
	var point := control.get_global_rect().get_center()
	var local := Vector3((point.x/surface.viewport.size.x-0.5)*surface.mesh.size.x,(0.5-point.y/surface.viewport.size.y)*surface.mesh.size.y,0.025)
	var target: Vector3=surface.to_global(local)
	if not scene.fps_active: return scene.camera.unproject_position(target)
	var direction: Vector3=(target-scene.camera.global_position).normalized()
	var yaw := atan2(-direction.x,-direction.z)
	var pitch := asin(direction.y)
	var motion := InputEventMouseMotion.new()
	motion.position=root.size*0.5
	motion.relative=Vector2(wrapf(scene.orbit_yaw-yaw,-PI,PI),scene.orbit_pitch-pitch)/0.003
	root.push_input(motion,true)
	await process_frame
	await process_frame
	return root.size*0.5

func click_control(scene, surface, control: Control) -> void:
	var captured: bool=scene.fps_active
	click(await aim_control(scene,surface,control))
	if captured and (not scene.fps_active or Input.mouse_mode!=Input.MOUSE_MODE_CAPTURED):
		push_error("FPS panel click released the mouse"); quit(1)

func add_component_ui(scene, surface, panel, type_id: String) -> void:
	if not panel.component_catalog_open:
		panel.scroll.ensure_control_visible(panel.component_catalog_toggle)
		await process_frame
		await click_control(scene,surface,panel.component_catalog_toggle)
	if not panel.component_catalog_buttons.has(type_id): push_error("Component missing from catalog: " + type_id); quit(1); return
	panel.scroll.ensure_control_visible(panel.component_catalog_buttons[type_id])
	await process_frame
	await click_control(scene,surface,panel.component_catalog_buttons[type_id])

func remove_component_ui(scene, surface, panel, type_id: String) -> void:
	if not panel.component_remove_buttons.has(type_id): push_error("Component missing Remove: " + type_id); quit(1); return
	panel.scroll.ensure_control_visible(panel.component_remove_buttons[type_id])
	await process_frame
	await click_control(scene,surface,panel.component_remove_buttons[type_id])

func changed_image_pixels(first: Image, second: Image) -> int:
	var count := 0
	for y in range(first.get_height()/8, first.get_height()*7/8, 4):
		for x in range(first.get_width()/4, first.get_width()*3/4, 4):
			var a: Color=first.get_pixel(x,y)
			var b: Color=second.get_pixel(x,y)
			if absf(a.r-b.r)+absf(a.g-b.g)+absf(a.b-b.b)>0.25: count+=1
	return count

func run() -> void:
	var scene=load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and scene.player_initialized,"Scene not ready"): quit(1); return
	click(scene.hud.edit_mode_buttons.edit.get_global_rect().get_center())
	if not scene.edit_mode: push_error("Creation tools require Edit mode"); quit(1); return
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
	# TPS uses the free pointer at the projected button position.
	scene.set_view_mode(0)
	await process_frame
	await click_control(scene,surface,panel.cards[1])
	if not await until(func(): return scene.objects[id].creation.shape=="sphere","TPS pointer did not select the button"): quit(1); return
	if Input.mouse_mode!=Input.MOUSE_MODE_VISIBLE: push_error("TPS panel click captured the mouse"); quit(1); return
	scene.set_view_mode(1)
	await process_frame
	# Holding the UI button must still let relative mouse motion aim the view.
	await aim_control(scene,surface,panel.cards[1])
	mouse(MOUSE_BUTTON_LEFT,true)
	var yaw_before: float=scene.orbit_yaw
	motion=InputEventMouseMotion.new()
	motion.position=root.size*0.5; motion.relative=Vector2(8,0)
	root.push_input(motion,true)
	mouse(MOUSE_BUTTON_LEFT,false)
	if is_equal_approx(scene.orbit_yaw,yaw_before) or not scene.fps_active: push_error("FPS UI drag stopped mouse-look"); quit(1); return
	var initial_count: int=scene.objects.values().filter(func(o): return o.visual.kind=="primitive").size()
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").replace(".png","-palette.png"))
	for index in PRIMITIVES.SHAPES.size():
		if index>0:
			scene.hud.open_creation()
			if not await until(func(): return scene.world_panels.panels.size()==1,"Creation panel did not reopen"): quit(1); return
			surface=scene.world_panels.panels.values()[0]
			panel=surface.content
			id=surface.panel_id
			await create_timer(0.15).timeout
		await click_control(scene,surface,panel.cards[index])
		if not await until(func(): return scene.objects[id].creation.shape==PRIMITIVES.SHAPES[index] and panel.selected==index,"3D selection was not shared"): quit(1); return
		await click_control(scene,surface,panel.create_button)
		if not await until(func(): return not scene.objects.has(id) and scene.objects.values().filter(func(o): return o.visual.kind=="primitive").size()==initial_count+index+1 and scene.current.visual.get("shape")==PRIMITIVES.SHAPES[index],"Primitive not created/selected: "+PRIMITIVES.NAMES[index]): quit(1); return
		var object: Dictionary=scene.current
		if object.visual.get("shape")!=PRIMITIVES.SHAPES[index] or scene.render_nodes[scene.selected_id].mesh.get_class()!=PRIMITIVES.create(PRIMITIVES.SHAPES[index]).get_class():
			push_error("Wrong procedural mesh: expected "+PRIMITIVES.SHAPES[index]+", got "+str(object.visual)+", pending="+str(scene.pending)); quit(1); return
		if not scene.first_person or scene.view_mode.selected!=1: push_error("Creation switched away from FPS"); quit(1); return
		if not scene.fps_active or Input.mouse_mode!=Input.MOUSE_MODE_CAPTURED: push_error("Creating an object released FPS mouse capture"); quit(1); return
	scene.hud.open_creation()
	if not await until(func(): return scene.world_panels.panels.size()==1,"Creation panel did not reopen for Camera"): quit(1); return
	surface=scene.world_panels.panels.values()[0]
	panel=surface.content
	await click_control(scene,surface,panel.camera_button)
	if not await until(func(): return scene.current.get("visual",{}).get("shape","")=="camera" and scene.world_panels.panels.is_empty(),"Camera preset was not created"): quit(1); return
	var camera_id: String=scene.selected_id
	if scene.objects[camera_id].visual.shape!="camera" or not scene.render_nodes[camera_id].has_node("Lens"):
		push_error("Camera item is missing its shared body or lens"); quit(1); return
	if not scene.avatars.has(scene.self_id) or not scene.avatars[scene.self_id].visible:
		push_error("Local avatar must stay visible to photo and mirror cameras in FPS"); quit(1); return
	var self_layer: int=scene.avatars[scene.self_id].SELF_AVATAR_LAYER
	if (scene.camera.cull_mask & self_layer) != 0 or (scene.camera_tool.eye.cull_mask & self_layer) == 0:
		push_error("FPS camera must hide only its own avatar layer"); quit(1); return
	for visual in scene.avatars[scene.self_id].find_children("*","VisualInstance3D",true,false):
		if visual.layers!=self_layer: push_error("Local avatar visual is not on the camera-only layer"); quit(1); return
	var self_avatar: Node3D=scene.avatars[scene.self_id]
	var previous_avatar_position: Vector3=self_avatar.global_position
	self_avatar.set_process(false)
	self_avatar.global_position=Vector3(100,0,100)
	var photo_eye: Camera3D=scene.camera_tool.eye
	photo_eye.global_position=Vector3(100,1.1,102.5)
	photo_eye.look_at(Vector3(100,1.1,100))
	photo_eye.current=true
	scene.camera_tool.viewport.render_target_update_mode=SubViewport.UPDATE_ALWAYS
	await create_timer(0.1).timeout
	var with_avatar: Image=scene.camera_tool.viewport.get_texture().get_image()
	photo_eye.cull_mask &= ~self_layer
	await create_timer(0.1).timeout
	var without_avatar: Image=scene.camera_tool.viewport.get_texture().get_image()
	photo_eye.cull_mask |= self_layer
	scene.camera_tool.viewport.render_target_update_mode=SubViewport.UPDATE_DISABLED
	self_avatar.global_position=previous_avatar_position
	self_avatar.set_process(true)
	var changed_pixels := changed_image_pixels(with_avatar,without_avatar)
	if changed_pixels<80:
		push_error("Camera viewport did not render the local avatar: "+str(changed_pixels)); quit(1); return
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--photo-dir="): scene.camera_tool.photo_directory=arg.trim_prefix("--photo-dir=")
	scene.release_cursor()
	scene.hud.set_dock_open(true,false)
	await process_frame
	await process_frame
	click(scene.hud.dock_buttons.camera.get_global_rect().get_center())
	if not scene.camera_tool.active or scene.fps_active or Input.mouse_mode!=Input.MOUSE_MODE_VISIBLE:
		push_error("Camera viewfinder did not release FPS input: active="+str(scene.camera_tool.active)+" fps="+str(scene.fps_active)+" mouse="+str(Input.mouse_mode)+" dock="+str(scene.hud.dock_holder.visible)); quit(1); return
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").replace(".png","-camera.png"))
	var camera_start: Vector3=scene.camera_tool.position
	var player_start: Vector3=scene.player_target_position
	var fly := InputEventKey.new()
	fly.physical_keycode=KEY_W; fly.pressed=true
	root.push_input(fly,true)
	await create_timer(0.25).timeout
	fly=fly.duplicate(); fly.pressed=false
	root.push_input(fly,true)
	if scene.camera_tool.position.distance_to(camera_start)<0.2 or scene.player_target_position.distance_to(player_start)>0.1:
		push_error("Drone movement did not stay separate from the player"); quit(1); return
	var view_center: Vector2=scene.camera_tool.view.get_global_rect().get_center()
	var turn := InputEventMouseButton.new()
	turn.button_index=MOUSE_BUTTON_RIGHT; turn.position=view_center; turn.pressed=true
	root.push_input(turn,true)
	var drone_yaw_before: float=scene.camera_tool.yaw
	var drag := InputEventMouseMotion.new()
	drag.position=view_center; drag.relative=Vector2(20,-8)
	root.push_input(drag,true)
	turn=turn.duplicate(); turn.pressed=false
	root.push_input(turn,true)
	if scene.camera_tool.looking or is_equal_approx(scene.camera_tool.yaw,drone_yaw_before):
		push_error("Drone right-drag did not rotate the camera"); quit(1); return
	if not await until(func(): return scene.remote_previews.has(camera_id),"Drone live preview did not reach the client"): quit(1); return
	click(scene.camera_tool.shutter_button.get_global_rect().get_center())
	if not await until(func(): return scene.camera_tool.captured_image!=null and not scene.camera_tool.captured_image.is_empty(),"Camera shutter did not capture the world"): quit(1); return
	click(scene.camera_tool.save_button.get_global_rect().get_center())
	var saved_photo: String=scene.camera_tool.status.text.trim_prefix("保存: ")
	if not FileAccess.file_exists(saved_photo): push_error("Camera did not save a local photo: "+scene.camera_tool.status.text); quit(1); return
	click(scene.camera_tool.share_button.get_global_rect().get_center())
	if not await until(func(): return scene.objects.values().any(func(o): return o.visual.kind=="image" and str(o.fields[0].value).begins_with("Photo ")),"Camera did not share an image item"): quit(1); return
	scene.camera_tool.park()
	if not await until(func(): return not scene.camera_tool.active and Vector3(scene.objects[camera_id].world_position[0],scene.objects[camera_id].world_position[1],scene.objects[camera_id].world_position[2]).distance_to(camera_start)>0.2,"Drone pose did not commit"): quit(1); return
	scene.hud.open_creation()
	if not await until(func(): return scene.world_panels.panels.size()==1,"Creation panel did not reopen for Mirror"): quit(1); return
	surface=scene.world_panels.panels.values()[0]
	panel=surface.content
	await click_control(scene,surface,panel.mirror_button)
	if not await until(func(): return scene.current.get("visual",{}).get("shape","")=="mirror" and scene.mirror_renderer.entries.has(scene.selected_id),"Mirror preset was not created"): quit(1); return
	var mirror_id: String=scene.selected_id
	var mirror_node: MeshInstance3D=scene.render_nodes[mirror_id]
	if mirror_node.mesh is not QuadMesh or mirror_node.layers!=scene.mirror_renderer.MIRROR_LAYER:
		push_error("Mirror has no isolated reflection surface"); quit(1); return
	scene.set_view_mode(2,false)
	var normal: Vector3=mirror_node.global_basis.z.normalized()
	scene.camera.global_position=mirror_node.global_position+normal*3.0
	scene.camera.look_at(mirror_node.global_position,mirror_node.global_basis.y.normalized())
	if (scene.mirror_renderer.entries[mirror_id].camera.cull_mask & self_layer) == 0:
		push_error("Mirror camera excluded the local avatar layer"); quit(1); return
	self_avatar.set_process(false)
	var mirror_avatar_position: Vector3=self_avatar.global_position
	self_avatar.global_position=mirror_node.global_position-Vector3(0,1.0,0)+normal*1.2
	await create_timer(0.2).timeout
	var avatar_reflected: Image=scene.mirror_renderer.entries[mirror_id].viewport.get_texture().get_image()
	scene.mirror_renderer.entries[mirror_id].camera.cull_mask &= ~self_layer
	await create_timer(0.2).timeout
	var no_avatar_reflected: Image=scene.mirror_renderer.entries[mirror_id].viewport.get_texture().get_image()
	scene.mirror_renderer.entries[mirror_id].camera.cull_mask |= self_layer
	self_avatar.global_position=mirror_avatar_position
	self_avatar.set_process(true)
	if changed_image_pixels(avatar_reflected,no_avatar_reflected)<60:
		push_error("Mirror viewport did not render the local avatar"); quit(1); return
	var red := MeshInstance3D.new()
	var red_mesh := SphereMesh.new()
	red_mesh.radius=0.23; red_mesh.height=0.46
	red.mesh=red_mesh
	var red_material := StandardMaterial3D.new()
	red_material.albedo_color=Color.RED
	red_material.shading_mode=BaseMaterial3D.SHADING_MODE_UNSHADED
	red.material_override=red_material
	scene.add_child(red)
	red.global_position=mirror_node.global_position+normal*1.2+mirror_node.global_basis.x.normalized()*0.5
	await create_timer(0.3).timeout
	var reflected: Image=scene.mirror_renderer.entries[mirror_id].viewport.get_texture().get_image()
	var has_red := false
	for y in range(0,reflected.get_height(),8):
		for x in range(0,reflected.get_width(),8):
			var pixel: Color=reflected.get_pixel(x,y)
			if pixel.r>0.65 and pixel.r>pixel.g*1.8 and pixel.r>pixel.b*1.8: has_red=true; break
		if has_red: break
	if not has_red: push_error("Mirror viewport did not reflect an object in front of it"); quit(1); return
	scene.camera.global_position=mirror_node.global_position-normal*3.0
	scene.camera.look_at(mirror_node.global_position,mirror_node.global_basis.y.normalized())
	red.global_position=mirror_node.global_position-normal*1.2+mirror_node.global_basis.x.normalized()*0.5
	await create_timer(0.2).timeout
	reflected=scene.mirror_renderer.entries[mirror_id].viewport.get_texture().get_image()
	has_red=false
	for y in range(0,reflected.get_height(),8):
		for x in range(0,reflected.get_width(),8):
			var pixel: Color=reflected.get_pixel(x,y)
			if pixel.r>0.65 and pixel.r>pixel.g*1.8 and pixel.r>pixel.b*1.8: has_red=true; break
		if has_red: break
	if not has_red: push_error("Mirror back face did not update its reflection"); quit(1); return
	scene.camera.global_position=mirror_node.global_position+normal*3.0
	scene.camera.look_at(mirror_node.global_position,mirror_node.global_basis.y.normalized())
	red.global_position=mirror_node.global_position+normal*1.2+mirror_node.global_basis.x.normalized()*0.5
	await process_frame
	var mirror_entry: Dictionary=scene.mirror_renderer.entries[mirror_id]
	var original_pose: Transform3D=mirror_node.global_transform
	scene.mirror_renderer.tick()
	var reflection_eye: Vector3=mirror_entry.camera.global_position
	scene.camera.global_position+=normal*0.2
	scene.mirror_renderer.tick()
	if mirror_entry.camera.global_position.distance_to(reflection_eye)<0.19:
		push_error("Static mirror did not refresh with the viewer in the same frame"); quit(1); return
	scene.camera.global_position-=normal*0.2
	scene.mirror_renderer.tick()
	reflection_eye=mirror_entry.camera.global_position
	mirror_node.global_position+=normal*0.2
	scene.mirror_renderer.tick()
	if mirror_entry.camera.global_position.distance_to(reflection_eye)<0.3:
		push_error("Moving mirror did not refresh in the same frame"); quit(1); return
	mirror_node.global_transform=original_pose
	scene.mirror_renderer.tick()
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").replace(".png","-mirror.png"))
	red.queue_free()
	scene.set_view_mode(1)
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot="))
	scene.hud.open_creation()
	if not await until(func(): return scene.world_panels.panels.size()==1,"Creation panel did not reopen for Slot"): quit(1); return
	surface=scene.world_panels.panels.values()[0]
	panel=surface.content
	id=surface.panel_id
	await click_control(scene,surface,panel.slot_button)
	if not await until(func(): return not scene.objects.has(id) and scene.current.get("visual",{}).get("kind")=="none","Empty Slot was not created"): push_error("Slot status="+panel.status.text+" current="+str(scene.current)+" pending="+str(scene.pending)); quit(1); return
	if scene.current.get("renderer")!=null or scene.render_nodes[scene.selected_id].mesh!=null or scene.current.get("collider")!=null:
		push_error("Empty Slot unexpectedly has a renderer or collider"); quit(1); return
	var slot_id: String=scene.selected_id
	scene.hud.open_creation()
	if not await until(func(): return scene.world_panels.panels.size()==1,"Creation panel did not reopen for standalone text"): quit(1); return
	surface=scene.world_panels.panels.values()[0]
	panel=surface.content
	id=surface.panel_id
	await create_timer(0.2).timeout
	await click_control(scene,surface,panel.text_button)
	if not await until(func(): return not scene.objects.has(id) and scene.current.get("world_text")!=null and scene.current.get("parent")==null,"Standalone text was not created"): push_error("Text status="+panel.status.text+" current="+str(scene.current.get("world_text"))+" panel="+str(scene.objects.has(id))); quit(1); return
	var text_id: String=scene.selected_id
	var text_q: Array=scene.objects[text_id].world_rotation
	var text_normal: Vector3=Quaternion(text_q[0],text_q[1],text_q[2],text_q[3])*Vector3.BACK
	if text_normal.dot((scene.camera.global_position-scene.render_nodes[text_id].global_position).normalized())<0.5: push_error("Standalone text faces away from its creator"); quit(1); return
	var text_direction: Vector3=(scene.render_nodes[text_id].global_position-scene.camera.global_position).normalized()
	var look := InputEventMouseMotion.new()
	look.position=root.size*0.5
	look.relative=Vector2(wrapf(scene.orbit_yaw-atan2(-text_direction.x,-text_direction.z),-PI,PI),scene.orbit_pitch-asin(text_direction.y))/0.003
	root.push_input(look,true)
	await process_frame
	await process_frame
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").replace(".png","-text.png"))
	scene.select_object(slot_id)
	scene.world_panels.open_inspector(slot_id)
	if not await until(func(): return scene.world_panels.panels.size()==1 and scene.world_panels.panels.values()[0].inspector_content,"Inspector did not open for Component"): quit(1); return
	surface=scene.world_panels.panels.values()[0]
	panel=surface.content
	if not await until(func(): return panel.target_id==slot_id and panel.component_catalog_toggle!=null,"Inspector did not show Add Component"): quit(1); return
	await process_frame
	await click_control(scene,surface,panel.component_catalog_toggle)
	if not panel.component_catalog_buttons.has("Mirror") or not panel.component_catalog_buttons["Mirror"].disabled:
		push_error("Mirror must require a flat mesh in the component catalog"); quit(1); return
	panel.component_search.text="cam"
	panel.filter_component_catalog("cam")
	if not panel.component_catalog_buttons["Camera"].get_parent().visible or panel.component_catalog_buttons["Mirror"].get_parent().visible:
		push_error("Component catalog search did not filter entries"); quit(1); return
	panel.component_search.text=""
	panel.filter_component_catalog("")
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await RenderingServer.frame_post_draw
			root.get_texture().get_image().save_png(arg.trim_prefix("--screenshot=").replace(".png","-components.png"))
	await add_component_ui(scene,surface,panel,"MeshRenderer")
	if not await until(func(): return scene.objects[slot_id].get("visual",{}).get("shape","")=="quad" and scene.render_nodes[slot_id].mesh!=null,"Quad Renderer Component did not attach"):
		push_error("Inspector status="+panel.message.text+" selected="+scene.selected_id); quit(1); return
	await add_component_ui(scene,surface,panel,"Mirror")
	if not await until(func(): return scene.objects[slot_id].get("mirror")==true and scene.mirror_renderer.entries.has(slot_id) and panel.controls.has("mirror_enabled"),"Mirror Component did not attach"): quit(1); return
	panel.scroll.ensure_control_visible(panel.controls.mirror_enabled[0])
	await process_frame
	await click_control(scene,surface,panel.controls.mirror_enabled[0])
	if not await until(func(): return scene.objects[slot_id].get("mirror")==false and not scene.mirror_renderer.entries.has(slot_id),"Mirror Enabled did not turn off"): quit(1); return
	if scene.render_nodes[slot_id].layers!=1 or scene.render_nodes[slot_id].material_override.albedo_texture!=null:
		push_error("Disabled Mirror kept its reflection texture"); quit(1); return
	panel.scroll.ensure_control_visible(panel.controls.mirror_enabled[0])
	await process_frame
	await click_control(scene,surface,panel.controls.mirror_enabled[0])
	if not await until(func(): return scene.objects[slot_id].get("mirror")==true and scene.mirror_renderer.entries.has(slot_id),"Mirror Enabled did not turn on"): quit(1); return
	if scene.render_nodes[slot_id].layers!=scene.mirror_renderer.MIRROR_LAYER:
		push_error("Enabled Mirror did not restore its reflection surface"); quit(1); return
	await remove_component_ui(scene,surface,panel,"Mirror")
	if not await until(func(): return scene.objects[slot_id].get("mirror")==null and not scene.mirror_renderer.entries.has(slot_id),"Mirror Component did not detach"): quit(1); return
	if not await until(func(): return panel.component_remove_buttons.has("MeshRenderer"),"Inspector did not show Remove MeshRenderer"): quit(1); return
	await remove_component_ui(scene,surface,panel,"MeshRenderer")
	if not await until(func(): return scene.objects[slot_id].visual.kind=="none" and scene.render_nodes[slot_id].mesh==null,"Renderer Component did not detach"): quit(1); return
	await add_component_ui(scene,surface,panel,"Camera")
	if not await until(func(): return scene.objects[slot_id].get("camera")==true and scene.camera_tool.is_camera(slot_id) and panel.controls.has("camera_enabled"),"Camera Component did not attach"): quit(1); return
	panel.scroll.ensure_control_visible(panel.controls.camera_enabled[0])
	await process_frame
	await click_control(scene,surface,panel.controls.camera_enabled[0])
	if not await until(func(): return scene.objects[slot_id].get("camera")==false and not scene.camera_tool.is_camera(slot_id),"Camera Enabled did not turn off"): quit(1); return
	panel.scroll.ensure_control_visible(panel.controls.camera_enabled[0])
	await process_frame
	await click_control(scene,surface,panel.controls.camera_enabled[0])
	if not await until(func(): return scene.objects[slot_id].get("camera")==true and scene.camera_tool.is_camera(slot_id),"Camera Enabled did not turn on"): quit(1); return
	await remove_component_ui(scene,surface,panel,"Camera")
	if not await until(func(): return scene.objects[slot_id].get("camera")==null,"Camera Component did not detach"): quit(1); return
	await add_component_ui(scene,surface,panel,"WorldText")
	if not await until(func(): return scene.objects[slot_id].get("world_text")!=null and scene.render_nodes[slot_id].has_node("WorldText"),"WorldText did not attach"):
		push_error("Inspector status="+panel.message.text+" value="+str(scene.objects[slot_id].get("world_text"))); quit(1); return
	if scene.render_nodes[slot_id].get_node("WorldText").text!="Text": push_error("Wrong WorldText content"); quit(1); return
	if not await until(func(): return panel.component_remove_buttons.has("WorldText"),"Inspector did not show Remove WorldText"): quit(1); return
	await remove_component_ui(scene,surface,panel,"WorldText")
	if not await until(func(): return scene.objects[slot_id].get("world_text")==null and not scene.render_nodes[slot_id].has_node("WorldText"),"WorldText did not detach"): quit(1); return
	await add_component_ui(scene,surface,panel,"WorldText")
	if not await until(func(): return scene.objects[slot_id].get("world_text")!=null and scene.render_nodes[slot_id].has_node("WorldText"),"WorldText did not reattach"): quit(1); return
	await click_control(scene,surface,panel.hierarchy.child_text_button)
	if not await until(func(): return scene.objects.values().any(func(o): return o.get("parent")!=null and o.parent.instance_id==slot_id and o.get("world_text")!=null),"Text child was not created"): quit(1); return
	var child_id: String=scene.selected_id
	if not await until(func(): return panel.target_id==child_id and not panel.target_change_pending,"Inspector did not select text child"): quit(1); return
	await create_timer(0.2).timeout
	scene.world_panels.close_panel(surface.panel_id)
	if not await until(func(): return scene.world_panels.panels.is_empty(),"Inspector did not close"): push_error("Panel IDs="+str(scene.world_panels.panels.keys())+" pending="+str(scene.world_panels.pending_panel_close)+" edits="+str(panel.edits)+" busy="+str(panel.script_view.busy())); quit(1); return
	scene.hud.open_creation()
	if not await until(func(): return scene.world_panels.panels.size()==1,"Creation panel did not reopen for Esc"): quit(1); return
	id=scene.world_panels.panels.keys()[0]
	var escape := InputEventKey.new()
	escape.keycode=KEY_ESCAPE; escape.pressed=true
	root.push_input(escape,true)
	if not scene.world_panels.panels.has(id) or Input.mouse_mode!=Input.MOUSE_MODE_VISIBLE: push_error("Esc deleted world panel or captured cursor"); quit(1); return
	print("CREATION SMOKE PASS: mirror reflection, drone preview and pose, local and shared photos, empty and text Slot presets, inspector component editing and text child")
	scene.queue_free()
	await process_frame
	await process_frame
	quit(0)
