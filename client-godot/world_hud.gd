extends CanvasLayer
## Local application chrome. The world and edit operations stay in the Engine.
const RING = preload("res://ring_menu.gd")
const INK := Color("101b25")
const LINE := Color("304550")
const TEXT := Color("e6f0f4")
const MUTED := Color("91a8b5")
const ACCENT := Color("77dbce")
var app
var root := Control.new()
var ring = RING.new()
var menu := ColorRect.new()
var workspace
var room_label: Label
var sync_label: Label
var fps_label: Label
var engine_label: Label
var profile_button: Button
var mode_button: Button
var home_button: Button
var menu_home_button: Button
var ring_target := ""
var last_overlay := false
var next_status := 0

func style(color: Color, padding: int = 12) -> StyleBoxFlat:
	var result := StyleBoxFlat.new()
	result.bg_color = color
	result.border_color = LINE
	result.set_border_width_all(1)
	result.set_corner_radius_all(8)
	result.set_content_margin_all(padding)
	return result

func text(value: String, size: int = 15, color: Color = TEXT) -> Label:
	var result := Label.new()
	result.text = value
	result.add_theme_font_size_override("font_size",size)
	result.add_theme_color_override("font_color",color)
	return result

func button(parent: Node, value: String, action: Callable) -> Button:
	var result := Button.new()
	result.text = value
	result.focus_mode = Control.FOCUS_NONE
	result.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	result.add_theme_color_override("font_color",TEXT)
	result.add_theme_stylebox_override("normal",style(Color.TRANSPARENT,8))
	result.add_theme_stylebox_override("hover",style(Color("233b47"),8))
	result.add_theme_stylebox_override("pressed",style(Color("2b5057"),8))
	result.pressed.connect(action)
	parent.add_child(result)
	return result

func setup(owner) -> void:
	app = owner
	workspace = app.get("service_panel")
	layer = 5
	add_child(root)
	root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var theme := Theme.new()
	theme.default_font_size = 15
	root.theme = theme
	var bar := PanelContainer.new()
	bar.set_anchors_and_offsets_preset(Control.PRESET_TOP_WIDE)
	bar.offset_left=16; bar.offset_right=-16; bar.offset_top=14; bar.offset_bottom=66
	bar.add_theme_stylebox_override("panel",style(Color("101b25f5"),8))
	root.add_child(bar)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation",12)
	bar.add_child(row)
	row.add_child(text("◇  kokoverse",21))
	row.add_child(VSeparator.new())
	room_label=text("Workshop",14,MUTED)
	room_label.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	room_label.text_overrun_behavior=TextServer.OVERRUN_TRIM_ELLIPSIS
	row.add_child(room_label)
	sync_label=text("● 接続待ち",13,MUTED)
	row.add_child(sync_label)
	engine_label=text("",12,MUTED)
	row.add_child(engine_label)
	fps_label=text("-- FPS",13)
	fps_label.custom_minimum_size.x=62
	row.add_child(fps_label)
	mode_button=button(row,"FPS",cycle_view)
	mode_button.tooltip_text="視点切り替え · F5"
	home_button=button(row,"ホーム / ライブラリ",open_home)
	profile_button=button(row,"メニュー",toggle_menu)
	profile_button.tooltip_text="メニュー · Esc"
	build_menu()
	root.add_child(ring)
	ring.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	ring.hide()
	ring.chosen.connect(choose_action)
	ring.cancelled.connect(close_ring)
	# The home entry now lives in the top bar and Esc menu.
	if workspace != null:
		workspace.open_button.hide()
		workspace.open_button.visibility_changed.connect(func():
			if workspace.open_button.visible: workspace.open_button.hide())

func build_menu() -> void:
	menu.color=Color(0.03,0.05,0.07,0.72)
	root.add_child(menu)
	menu.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	menu.hide()
	var center := CenterContainer.new()
	menu.add_child(center)
	center.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	var shell := PanelContainer.new()
	shell.custom_minimum_size=Vector2(700,460)
	shell.add_theme_stylebox_override("panel",style(INK,24))
	center.add_child(shell)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation",18)
	shell.add_child(column)
	var header := HBoxContainer.new()
	column.add_child(header)
	var title := text("メニュー",23)
	title.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	header.add_child(title)
	button(header,"ワールドへ戻る",close_menu)
	column.add_child(HSeparator.new())
	var body := HBoxContainer.new()
	body.add_theme_constant_override("separation",28)
	column.add_child(body)
	var nav := VBoxContainer.new()
	nav.custom_minimum_size.x=180
	nav.add_theme_constant_override("separation",12)
	body.add_child(nav)
	menu_home_button=button(nav,"ホーム / ライブラリ",open_home)
	var settings_button := button(nav,"ユーザー設定",func(): pass)
	settings_button.add_theme_color_override("font_color",ACCENT)
	var settings := VBoxContainer.new()
	settings.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	settings.add_theme_constant_override("separation",12)
	body.add_child(settings)
	settings.add_child(text("ユーザー設定",20))
	settings.add_child(text("表示名",13,MUTED))
	app.nickname.reparent(settings)
	app.nickname.custom_minimum_size.y=38
	settings.add_child(text("アバター",13,MUTED))
	var avatar_row := HBoxContainer.new()
	settings.add_child(avatar_row)
	app.avatar_button.reparent(avatar_row)
	app.avatar_button.text="テクスチャを選択"
	app.reset_avatar.reparent(avatar_row)
	app.reset_avatar.text="リセット"
	settings.add_child(HSeparator.new())
	settings.add_child(text("参加者",13,MUTED))
	app.people_label.reparent(settings)
	app.people_label.max_lines_visible=6

func workspace_open() -> bool:
	return workspace != null and workspace.enabled and workspace.shade.visible

func blocking() -> bool:
	return menu.visible or ring.visible or workspace_open()

func resume_view() -> void:
	if blocking() or app.world_panels.has_window_focus() or not app.get_window().has_focus(): return
	if app.avatar_camera and app.first_person:
		app.fps_active=true
		Input.mouse_mode=Input.MOUSE_MODE_CAPTURED

func close_menu() -> void:
	menu.hide()
	resume_view()

func toggle_menu() -> void:
	if menu.visible: close_menu()
	else:
		ring.hide()
		app.get_viewport().gui_release_focus()
		app.world_panels.clear_focus()
		app.release_cursor()
		menu.show()

func open_home() -> void:
	if workspace == null or not workspace.enabled: return
	menu.hide()
	ring.hide()
	workspace.show_panel()

func close_ring() -> void:
	ring.hide()
	resume_view()

func cycle_view() -> void:
	app.set_view_mode((app.view_mode.selected+1)%3)
	if blocking(): app.release_cursor()

func show_ring(position: Vector2) -> void:
	var screen: Vector2 = app.get_viewport().get_visible_rect().size*0.5 if app.fps_active else position
	ring_target=app.selected_id
	var origin: Vector3=app.camera.project_ray_origin(screen)
	var query := PhysicsRayQueryParameters3D.create(origin,origin+app.camera.project_ray_normal(screen)*1000)
	var hit: Dictionary=app.get_world_3d().direct_space_state.intersect_ray(query)
	if not hit.is_empty() and hit.collider.has_meta("object_id"):
		ring_target=str(hit.collider.get_meta("object_id"))
		app.select_object(ring_target)
	var name_text: String=str(app.world_panels.field_data(ring_target,"name").get("value","World"))
	app.world_panels.clear_focus()
	app.release_cursor()
	ring.open(screen,name_text,app.editable)

func choose_action(index: int) -> void:
	ring.hide()
	match index:
		0: app.world_panels.open_inspector(ring_target,true)
		1: app.world_panels.open_inspector(ring_target)
		2: app.submit_structure("create")
		3: app.choose_image()
	if index in [1,2]: resume_view()

func handle_input(event: InputEvent) -> bool:
	if event is InputEventKey and event.pressed and not event.echo:
		if event.keycode == KEY_ESCAPE or event.physical_keycode == KEY_ESCAPE:
			if workspace_open(): workspace.shade.hide(); resume_view()
			elif ring.visible: close_ring()
			else: toggle_menu()
			return true
		if (event.keycode == KEY_F5 or event.physical_keycode == KEY_F5) and not app.gameplay_keyboard_blocked():
			cycle_view()
			return true
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_MIDDLE and event.pressed:
		if ring.visible: close_ring(); return true
		if not app.gameplay_keyboard_blocked(): show_ring(event.position); return true
	return false

func _process(_delta: float) -> void:
	var overlay := blocking()
	if last_overlay and not overlay: resume_view()
	last_overlay=overlay
	if Time.get_ticks_msec()<next_status: return
	next_status=Time.get_ticks_msec()+200
	fps_label.text="%d FPS" % Engine.get_frames_per_second()
	mode_button.text=["Third person","FPS","Fly camera"][app.view_mode.selected]
	var ready: bool=app.connected and app.editable
	var busy: bool=app.pending
	for request in app.requests.requests.values():
		busy=busy or request.kind in ["edit","structure","transform","script","playback"]
	for view in app.world_panels.views():
		busy=busy or view.edits.values().any(func(edit): return edit.dirty or edit.inflight) or not view.script_view.flight.is_empty()
	sync_label.text="● 同期中" if ready and busy else ("● 同期済み" if ready else "● 接続待ち")
	sync_label.add_theme_color_override("font_color",ACCENT if ready else Color("e9bc70"))
	sync_label.tooltip_text=app.status_label.text
	engine_label.text="Engine %d ms" % roundi(app.clock_best_rtt*1000) if app.clock_ready else ""
	engine_label.tooltip_text="ローカルEngineとの往復時間"
	room_label.text=str(workspace.current_session.get("name",app.session)) if workspace != null else app.session
	home_button.visible=workspace != null and workspace.enabled
	menu_home_button.disabled=not home_button.visible
