extends MeshInstance3D
## A local, temporary world-space creation palette; spawned objects belong to Rust.
signal closed
const PRIMITIVES = preload("res://primitive_mesh.gd")
const PIXELS := Vector2i(1120,720)
const METERS := Vector2(2.4, 2.4*720.0/1120.0)
var app
var viewport := SubViewport.new()
var content := PanelContainer.new()
var cards: Array[Button] = []
var selected := 0
var create_button: Button
var status: Label
var busy := false
var last_pointer := Vector2(-1000,-1000)

func label(value: String, size: int = 20) -> Label:
	var result := Label.new()
	result.text=value
	result.add_theme_font_size_override("font_size",size)
	return result

func setup(owner) -> void:
	app=owner
	mesh=QuadMesh.new()
	mesh.size=METERS
	viewport.size=PIXELS
	viewport.disable_3d=true
	viewport.gui_embed_subwindows=true
	viewport.render_target_update_mode=SubViewport.UPDATE_ALWAYS
	add_child(viewport)
	viewport.add_child(content)
	content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	content.theme=Theme.new()
	content.theme.default_font_size=20
	var style := StyleBoxFlat.new()
	style.bg_color=Color("14222f")
	style.border_color=Color("5cbcae")
	style.set_border_width_all(2)
	style.set_content_margin_all(24)
	content.add_theme_stylebox_override("panel",style)
	var material := StandardMaterial3D.new()
	material.shading_mode=BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_texture=viewport.get_texture()
	material.texture_filter=BaseMaterial3D.TEXTURE_FILTER_LINEAR
	material.no_depth_test=true
	material.render_priority=100
	material_override=material
	global_transform=Transform3D(app.camera.global_basis.orthonormalized(),app.camera.global_position-app.camera.global_basis.z*2.6-app.camera.global_basis.x*0.3)
	build_ui()

func build_ui() -> void:
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation",16)
	content.add_child(column)
	var header := HBoxContainer.new()
	column.add_child(header)
	var title := label("新規作成",28)
	title.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	header.add_child(title)
	var close := Button.new()
	close.text="×"
	close.focus_mode=Control.FOCUS_NONE
	close.custom_minimum_size=Vector2(48,44)
	close.pressed.connect(func(): closed.emit())
	header.add_child(close)
	column.add_child(HSeparator.new())
	var body := HBoxContainer.new()
	body.size_flags_vertical=Control.SIZE_EXPAND_FILL
	body.add_theme_constant_override("separation",22)
	column.add_child(body)
	var categories := VBoxContainer.new()
	categories.custom_minimum_size.x=155
	body.add_child(categories)
	categories.add_child(label("3Dモデル",23))
	categories.add_child(label("Procedural",15))
	var grid := GridContainer.new()
	grid.columns=4
	grid.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	grid.add_theme_constant_override("h_separation",12)
	grid.add_theme_constant_override("v_separation",12)
	body.add_child(grid)
	var group := ButtonGroup.new()
	for index in PRIMITIVES.SHAPES.size():
		var card := Button.new()
		card.toggle_mode=true
		card.button_group=group
		card.focus_mode=Control.FOCUS_NONE
		card.custom_minimum_size=Vector2(188,220)
		card.pressed.connect(select_shape.bind(index))
		card.gui_input.connect(func(event):
			if event is InputEventMouseButton and event.button_index==MOUSE_BUTTON_LEFT and event.double_click and event.pressed:
				select_shape(index)
				spawn_selected())
		grid.add_child(card)
		cards.append(card)
		var stack := VBoxContainer.new()
		card.add_child(stack)
		stack.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		stack.offset_left=8; stack.offset_right=-8; stack.offset_top=8; stack.offset_bottom=-8
		stack.mouse_filter=Control.MOUSE_FILTER_IGNORE
		var preview := SubViewportContainer.new()
		preview.custom_minimum_size=Vector2(160,166)
		preview.size_flags_vertical=Control.SIZE_EXPAND_FILL
		preview.stretch=true
		preview.mouse_filter=Control.MOUSE_FILTER_IGNORE
		stack.add_child(preview)
		build_preview(preview,PRIMITIVES.SHAPES[index])
		var name_label := label(PRIMITIVES.NAMES[index],19)
		name_label.horizontal_alignment=HORIZONTAL_ALIGNMENT_CENTER
		name_label.mouse_filter=Control.MOUSE_FILTER_IGNORE
		stack.add_child(name_label)
	column.add_child(HSeparator.new())
	var footer := HBoxContainer.new()
	column.add_child(footer)
	status=label("",18)
	status.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	footer.add_child(status)
	create_button=Button.new()
	create_button.custom_minimum_size=Vector2(210,48)
	create_button.focus_mode=Control.FOCUS_NONE
	create_button.pressed.connect(spawn_selected)
	footer.add_child(create_button)
	select_shape(0)

func build_preview(parent: SubViewportContainer, shape: String) -> void:
	var preview := SubViewport.new()
	preview.size=Vector2i(160,166)
	preview.own_world_3d=true
	preview.render_target_update_mode=SubViewport.UPDATE_ONCE
	parent.add_child(preview)
	var camera := Camera3D.new()
	preview.add_child(camera)
	camera.position=Vector3(2,1.6,2.8)
	camera.look_at(Vector3.ZERO)
	camera.projection=Camera3D.PROJECTION_ORTHOGONAL
	camera.size=2.2
	var environment := WorldEnvironment.new()
	environment.environment=Environment.new()
	environment.environment.background_mode=Environment.BG_COLOR
	environment.environment.background_color=Color("1b2c39")
	environment.environment.ambient_light_source=Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color=Color.WHITE
	environment.environment.ambient_light_energy=0.7
	preview.add_child(environment)
	var light := DirectionalLight3D.new()
	light.rotation_degrees=Vector3(-45,-30,0)
	preview.add_child(light)
	var object := MeshInstance3D.new()
	object.mesh=PRIMITIVES.create(shape)
	var material := StandardMaterial3D.new()
	material.albedo_color=Color("7ed8cf")
	material.roughness=0.55
	material.cull_mode=BaseMaterial3D.CULL_DISABLED
	object.material_override=material
	preview.add_child(object)

func select_shape(index: int) -> void:
	selected=index
	cards[index].button_pressed=true
	create_button.text=PRIMITIVES.NAMES[index]+" を作成"

func spawn_selected() -> void:
	if busy or not app.editable: return
	busy=true
	create_button.disabled=true
	status.text="作成中…"
	var shape: String=PRIMITIVES.SHAPES[selected]
	var name_text: String=PRIMITIVES.NAMES[selected]
	var position: Vector3=app.camera.global_position-app.camera.global_basis.z*4.0+app.camera.global_basis.x*2.2
	position.y=app.player_position.y+(0.03 if shape=="grid" else 0.75)
	app.world_panels.command({"kind":"structure","command":{"action":"import_asset","name":name_text,"position":[position.x,position.y,position.z],"rotation":[0,0,0,1],"visual":{"kind":"primitive","shape":shape}}},func(result):
		busy=false
		create_button.disabled=false
		if result.ok:
			app.select_object(result.selected.instance_id)
			status.text=name_text+" を追加しました"
		else: status.text=result.get("message","作成できませんでした"))

func route_input(event: InputEvent) -> bool:
	if not event is InputEventMouse: return false
	var origin: Vector3=app.camera.project_ray_origin(event.position)
	var direction: Vector3=app.camera.project_ray_normal(event.position)
	var hit: Variant=Plane(global_basis.z.normalized(),global_position).intersects_ray(origin,direction)
	var point := Vector2(-1000,-1000)
	if hit != null and direction.dot(global_basis.z)<0:
		var local := to_local(hit)
		if absf(local.x)<=METERS.x*0.5 and absf(local.y)<=METERS.y*0.5:
			point=Vector2((local.x/METERS.x+0.5)*PIXELS.x,(0.5-local.y/METERS.y)*PIXELS.y)
	var copy: InputEventMouse=event.duplicate()
	copy.position=point
	copy.global_position=point
	if copy is InputEventMouseMotion: copy.relative=point-last_pointer
	last_pointer=point
	viewport.push_input(copy,true)
	return true
