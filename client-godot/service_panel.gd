extends CanvasLayer
## HN home, saved items and workshop. The Engine keeps session credentials.
const Art = preload("res://service_art.gd")
const PAPER = Color("fffdf9")
const GROUND = Color("eae5db")
const INK = Color("26262e")
const MUTED = Color("6b6872")
const LINE = Color("dcd5c8")
const BLUE = Color("2f5fc4")
const PALE = Color("eef3fc")
var app
var shade: ColorRect
var shell: PanelContainer
var status: Label
var identity: Label
var name_input: LineEdit
var session_name: LineEdit
var sessions: VBoxContainer
var library: GridContainer
var workshop: GridContainer
var current_user: Dictionary = {}
var current_session: Dictionary = {}
var enabled := false
var auto_login := false
var open_button: Button
var library_offset := 0
var workshop_offset := 0
var load_world := ""
var world_choice: Label
var library_query: LineEdit
var workshop_query: LineEdit
var mine_only: CheckBox
var page := "sessions"
var pages := {}
var nav := {}
var heading: Label
var subtitle: Label
var detail_box: VBoxContainer
var detail_panel: PanelContainer
var selected := {}
var selection_kind := ""
var library_total := 0
var workshop_total := 0
var library_page: Label
var workshop_page: Label
var save_button: Button
var login_button: Button
var back_button: Button
var library_generation := 0
var workshop_generation := 0
var detail_generation := 0
var body_font: SystemFont
var title_font: SystemFont
var publish_dialog: ConfirmationDialog
var publish_title: LineEdit
var publish_description: TextEdit
var publish_error: Label
var publishing_version := ""

func box(color: Color, radius: int = 12, border: Color = Color.TRANSPARENT, padding: int = 16) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color=color
	style.set_corner_radius_all(radius)
	style.border_color=border
	style.set_border_width_all(1 if border.a>0 else 0)
	style.content_margin_left=padding
	style.content_margin_right=padding
	style.content_margin_top=padding
	style.content_margin_bottom=padding
	return style

func text(value: String, size: int = 14, color: Color = INK, strong: bool = false) -> Label:
	var label := Label.new()
	label.text=value
	label.add_theme_font_override("font",title_font if strong else body_font)
	label.add_theme_font_size_override("font_size",size)
	label.add_theme_color_override("font_color",color)
	return label

func paragraph(value: String, size: int = 14, color: Color = MUTED) -> Label:
	var label := text(value,size,color)
	label.autowrap_mode=TextServer.AUTOWRAP_WORD_SMART
	label.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	return label

func row(parent: Node, gap: int = 10) -> HBoxContainer:
	var node := HBoxContainer.new()
	node.add_theme_constant_override("separation",gap)
	parent.add_child(node)
	return node

func column(parent: Node, gap: int = 10) -> VBoxContainer:
	var node := VBoxContainer.new()
	node.add_theme_constant_override("separation",gap)
	parent.add_child(node)
	return node

func button(parent: Node, value: String, callback: Callable, primary: bool = false) -> Button:
	var b := Button.new()
	b.text=value
	b.custom_minimum_size.y=38
	b.mouse_default_cursor_shape=Control.CURSOR_POINTING_HAND
	b.add_theme_stylebox_override("normal",box(BLUE if primary else PAPER,9,BLUE if primary else LINE,12))
	b.add_theme_stylebox_override("hover",box(Color("274fa5") if primary else PALE,9,BLUE,12))
	b.add_theme_stylebox_override("pressed",box(Color("23478f") if primary else Color("e1e9f8"),9,BLUE,12))
	b.add_theme_stylebox_override("disabled",box(Color("f2eee7"),9,LINE,12))
	b.add_theme_color_override("font_color",Color.WHITE if primary else INK)
	b.add_theme_color_override("font_hover_color",Color.WHITE if primary else BLUE)
	b.add_theme_color_override("font_pressed_color",Color.WHITE if primary else BLUE)
	b.add_theme_color_override("font_disabled_color",Color("928c84"))
	b.pressed.connect(callback)
	parent.add_child(b)
	return b

func field(parent: Node, placeholder: String) -> LineEdit:
	var input := LineEdit.new()
	input.placeholder_text=placeholder
	input.custom_minimum_size.y=40
	input.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	parent.add_child(input)
	return input

func surface(parent: Node, color: Color = PAPER, padding: int = 16) -> PanelContainer:
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel",box(color,14,Color.TRANSPARENT,padding))
	parent.add_child(panel)
	return panel

func rule(parent: Node) -> void:
	var separator := HSeparator.new()
	separator.add_theme_stylebox_override("separator",box(LINE,0,Color.TRANSPARENT,0))
	separator.custom_minimum_size.y=1
	parent.add_child(separator)

func setup(scene) -> void:
	app=scene
	layer=20
	body_font=SystemFont.new()
	body_font.font_names=PackedStringArray(["Yu Gothic UI","Meiryo","Segoe UI"])
	title_font=SystemFont.new()
	title_font.font_names=PackedStringArray(["Yu Gothic","Meiryo","Segoe UI"])
	title_font.font_weight=700
	open_button=button(self,"ホーム / ライブラリ",show_panel)
	open_button.position=Vector2(280,112)
	open_button.visible=false
	shade=ColorRect.new()
	shade.color=Color(0.10,0.11,0.14,0.72)
	shade.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	shade.visible=false
	add_child(shade)
	shell=PanelContainer.new()
	shell.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	shell.offset_left=24
	shell.offset_right=-24
	shell.offset_top=20
	shell.offset_bottom=-20
	shell.add_theme_stylebox_override("panel",box(GROUND,20,Color.TRANSPARENT,16))
	var theme := Theme.new()
	theme.default_font=body_font
	theme.default_font_size=14
	for type in ["Label","LineEdit","TextEdit","CheckBox","Button"]:
		theme.set_color("font_color",type,INK)
		theme.set_color("font_hover_color",type,BLUE)
		theme.set_color("font_focus_color",type,BLUE)
		theme.set_color("font_pressed_color",type,INK)
		theme.set_color("font_disabled_color",type,MUTED)
	theme.set_color("font_uneditable_color","LineEdit",MUTED)
	theme.set_stylebox("normal","Button",box(PAPER,9,LINE,12))
	theme.set_stylebox("hover","Button",box(PALE,9,BLUE,12))
	theme.set_stylebox("pressed","Button",box(PALE,9,BLUE,12))
	theme.set_stylebox("disabled","Button",box(GROUND,9,LINE,12))
	theme.set_stylebox("normal","LineEdit",box(PAPER,10,LINE,10))
	theme.set_stylebox("focus","LineEdit",box(PAPER,10,BLUE,10))
	theme.set_color("font_placeholder_color","LineEdit",MUTED)
	theme.set_stylebox("normal","TextEdit",box(PAPER,10,LINE,10))
	theme.set_stylebox("focus","TextEdit",box(PAPER,10,BLUE,10))
	theme.set_stylebox("focus","Button",box(Color(0,0,0,0),9,BLUE,2))
	theme.set_stylebox("panel","AcceptDialog",box(PAPER,16,LINE,20))
	theme.set_color("title_color","Window",INK)
	theme.set_stylebox("embedded_border","Window",box(GROUND,14,LINE,12))
	shell.theme=theme
	shade.add_child(shell)
	var layout := row(shell,18)
	var side := surface(layout)
	side.custom_minimum_size.x=184
	var sidebar := column(side,12)
	sidebar.add_child(text("Kokoverse",24,INK,true))
	sidebar.add_child(text("ワークスペース",12,MUTED))
	rule(sidebar)
	for item in [["sessions","セッション"],["library","マイライブラリ"],["workshop","ワークショップ"]]:
		var key: String=item[0]
		nav[key]=button(sidebar,item[1],func(): switch_page(key))
		nav[key].alignment=HORIZONTAL_ALIGNMENT_LEFT
	var space := Control.new()
	space.size_flags_vertical=Control.SIZE_EXPAND_FILL
	sidebar.add_child(space)
	identity=paragraph("HNを入力してはじめる",13)
	sidebar.add_child(identity)
	name_input=field(sidebar,"HN")
	name_input.max_length=40
	login_button=button(sidebar,"Start",login,true)
	name_input.text_submitted.connect(func(_value): login())
	button(sidebar,"ユーザーコードをコピー",func():
		if not current_user.is_empty():
			DisplayServer.clipboard_set(str(current_user.id))
			status.text="ユーザーコードをコピーしました。"
	).tooltip_text="ワークショップを使わず直接渡すときに使います。"
	var main := column(layout,14)
	main.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	var top := row(main)
	var titles := column(top,3)
	titles.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	heading=text("セッション",27,INK,true)
	titles.add_child(heading)
	subtitle=paragraph("一緒に作る場所を選ぶ。",13)
	titles.add_child(subtitle)
	back_button=button(top,"ワールドへ戻る",func(): if not current_session.is_empty(): shade.hide())
	back_button.visible=false
	var workspace := row(main,16)
	workspace.size_flags_vertical=Control.SIZE_EXPAND_FILL
	var content := Control.new()
	content.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	workspace.add_child(content)
	for key in ["sessions","library","workshop"]:
		var pane := VBoxContainer.new()
		pane.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		pane.add_theme_constant_override("separation",12)
		content.add_child(pane)
		pages[key]=pane
	build_sessions(pages.sessions)
	build_library(pages.library)
	build_workshop(pages.workshop)
	detail_panel=surface(workspace)
	detail_panel.custom_minimum_size.x=292
	var detail_scroll := ScrollContainer.new()
	detail_scroll.horizontal_scroll_mode=ScrollContainer.SCROLL_MODE_DISABLED
	detail_panel.add_child(detail_scroll)
	detail_box=column(detail_scroll,12)
	detail_box.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	detail_box.custom_minimum_size.x=252
	status=paragraph("HNを入力してStartを押してください。同じHNなら同じライブラリを開けます。",12)
	main.add_child(status)
	content.resized.connect(update_columns)
	switch_page("sessions",false)

func build_sessions(parent: VBoxContainer) -> void:
	var panel := surface(parent)
	var form := column(panel)
	form.add_child(text("新しいセッション",17,INK,true))
	var controls := row(form)
	session_name=field(controls,"セッション名")
	session_name.max_length=80
	button(controls,"作成",create_session,true)
	var choices := row(form)
	button(choices,"保存ワールドから…",choose_world)
	button(choices,"新しいワールド",func(): load_world=""; world_choice.text="新しいワールドで開始")
	world_choice=paragraph("新しいワールドで開始",12)
	form.add_child(world_choice)
	var heading_row := row(parent)
	var title := text("参加できるセッション",16,INK,true)
	title.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	heading_row.add_child(title)
	button(heading_row,"更新",refresh_sessions)
	button(heading_row,"退出",func(): call_service("leave_session",{},func(_v): refresh_sessions()))
	sessions=scrolling_column(parent)

func build_library(parent: VBoxContainer) -> void:
	var tools := row(parent)
	save_button=button(tools,"選択中のアイテムを保存",save_selected,true)
	button(tools,"更新",refresh_library)
	library_query=field(parent,"ライブラリを検索")
	library_query.text_submitted.connect(func(_value): library_offset=0; refresh_library())
	library=scrolling_grid(parent)
	var pager := row(parent)
	button(pager,"前へ",func(): library_offset=maxi(0,library_offset-10); refresh_library())
	library_page=text("0 件",12,MUTED)
	library_page.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	library_page.horizontal_alignment=HORIZONTAL_ALIGNMENT_CENTER
	pager.add_child(library_page)
	button(pager,"次へ",func():
		if library_offset+10<library_total: library_offset+=10; refresh_library()
	)

func build_workshop(parent: VBoxContainer) -> void:
	var tools := row(parent)
	workshop_query=field(tools,"作品名・作者・説明を検索")
	workshop_query.text_submitted.connect(func(_value): workshop_offset=0; refresh_workshop())
	button(tools,"検索",func(): workshop_offset=0; refresh_workshop())
	button(tools,"更新",refresh_workshop)
	mine_only=CheckBox.new()
	mine_only.text="自分の公開作品"
	mine_only.toggled.connect(func(_on): workshop_offset=0; refresh_workshop())
	parent.add_child(mine_only)
	workshop=scrolling_grid(parent)
	var pager := row(parent)
	button(pager,"前へ",func(): workshop_offset=maxi(0,workshop_offset-12); refresh_workshop())
	workshop_page=text("0 件",12,MUTED)
	workshop_page.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	workshop_page.horizontal_alignment=HORIZONTAL_ALIGNMENT_CENTER
	pager.add_child(workshop_page)
	button(pager,"次へ",func():
		if workshop_offset+12<workshop_total: workshop_offset+=12; refresh_workshop()
	)

func scrolling_column(parent: Node) -> VBoxContainer:
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical=Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode=ScrollContainer.SCROLL_MODE_DISABLED
	parent.add_child(scroll)
	var list := column(scroll,10)
	list.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	return list

func scrolling_grid(parent: Node) -> GridContainer:
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical=Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode=ScrollContainer.SCROLL_MODE_DISABLED
	parent.add_child(scroll)
	var grid := GridContainer.new()
	grid.columns=2
	grid.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	grid.add_theme_constant_override("h_separation",12)
	grid.add_theme_constant_override("v_separation",12)
	scroll.add_child(grid)
	return grid

func update_columns() -> void:
	for grid in [library,workshop]:
		if is_instance_valid(grid): grid.columns=clampi(int(grid.get_parent().size.x/195.0),1,3)

func switch_page(value: String, refresh_data: bool = true) -> void:
	page=value
	for key in pages:
		pages[key].visible=key==page
		nav[key].add_theme_stylebox_override("normal",box(PALE if key==page else PAPER,10,Color.TRANSPARENT,12))
		nav[key].add_theme_color_override("font_color",BLUE if key==page else INK)
	detail_panel.visible=page!="sessions"
	heading.text={"sessions":"セッション","library":"マイライブラリ","workshop":"ワークショップ"}[page]
	subtitle.text={"sessions":"一緒に作る場所を選ぶ。","library":"保存した作品を配置する、共有する、公開する。","workshop":"みんなが公開した作品を探す。"}[page]
	selected={}
	selection_kind=""
	detail_generation+=1
	show_detail_empty()
	update_columns.call_deferred()
	if refresh_data and not current_user.is_empty(): refresh()

func login() -> void:
	if not current_session.is_empty(): status.text="HNを切り替えるには、セッションから退出してください。"; return
	if name_input.text.strip_edges().is_empty(): status.text="HNを入力してください。"; name_input.grab_focus(); return
	auto_login=true
	call_service("login",{"name":name_input.text},func(_v): selected={}; show_detail_empty(); refresh())

func receive_state(body: Dictionary) -> void:
	enabled=body.get("enabled",false)
	open_button.visible=enabled
	if not enabled: return
	current_user=body.user if body.get("user") is Dictionary else {}
	current_session=body.session if body.get("session") is Dictionary else {}
	back_button.visible=not current_session.is_empty()
	name_input.editable=current_session.is_empty()
	login_button.disabled=not current_session.is_empty()
	if not current_user.is_empty():
		identity.text="HN  "+str(current_user.name)
		name_input.text=current_user.name
		login_button.text="HNを切り替える"
		if not auto_login:
			auto_login=true
			call_service("login",{"name":current_user.name},func(_v): refresh())
	if current_session.is_empty(): show_panel()
	else: shade.hide()

func show_panel() -> void:
	app.release_cursor()
	shade.show()
	save_button.disabled=app.current.is_empty() or current_session.is_empty()
	save_button.tooltip_text="ワールドでアイテムを選択してから保存します。" if save_button.disabled else "見た目・設定・ロジックを保存します。"
	if not current_user.is_empty(): refresh()

func call_service(action: String, values: Dictionary, completed: Callable, failed: Callable = Callable()) -> void:
	if not app.connected: status.text="接続を待っています…"; return
	status.text="読み込み中…" if action in ["library","workshop","workshop_detail","version","sessions"] else "処理しています…"
	var body := values.duplicate(true)
	body.kind="service"
	body.action=action
	if action=="create_session" and not load_world.is_empty(): body.load_world=load_world
	app.requests.request(body,func(result):
		if not result.get("ok",false):
			status.text=result.get("message","処理できませんでした。もう一度お試しください。")
			if failed.is_valid(): failed.call(status.text)
			return
		status.text=""
		completed.call(result.get("data",{}))
	,["request_id"],"request_id",120000,"service_result")

func refresh() -> void:
	if page=="sessions": refresh_sessions()
	elif page=="library": refresh_library()
	else: refresh_workshop()

func create_session() -> void:
	if session_name.text.strip_edges().is_empty(): status.text="セッション名を入力してください。"; session_name.grab_focus(); return
	call_service("create_session",{"name":session_name.text},func(_v): pass)

func refresh_sessions() -> void:
	if current_user.is_empty(): return
	call_service("sessions",{},func(rows):
		clear_list(sessions)
		for record in rows:
			var card := surface(sessions)
			var contents := row(card)
			var labels := column(contents,3)
			labels.size_flags_horizontal=Control.SIZE_EXPAND_FILL
			labels.add_child(text(record.name,17,INK,true))
			labels.add_child(text("ホスト  "+str(record.owner_name),12,MUTED))
			var here: bool=current_session.get("id","")==record.id
			button(contents,"ワールドへ" if here else "参加",func():
				if here: shade.hide()
				elif not current_session.is_empty(): status.text="今のセッションから退出してから参加してください。"
				else: call_service("join_session",{"session_id":record.id},func(_v): pass)
			,true)
		if rows.is_empty(): empty_state(sessions,"まだセッションがありません","上のフォームから、新しいセッションを作成できます。")
	)

func clear_list(parent: Node) -> void:
	for child in parent.get_children(): parent.remove_child(child); child.queue_free()

func empty_state(parent: Node, title: String, hint: String) -> void:
	var panel := surface(parent)
	panel.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	var content := column(panel,8)
	content.add_child(paragraph(title,16,INK))
	content.add_child(paragraph(hint,13))

func refresh_library() -> void:
	if current_user.is_empty(): return
	library_generation+=1
	var generation := library_generation
	call_service("library",{"offset":library_offset,"query":library_query.text},func(data):
		if generation!=library_generation: return
		clear_list(library)
		library_total=int(data.get("total",0))
		library_page.text=page_label(library_offset,10,library_total)
		for record in data.get("entries",[]): item_card(library,record,"library")
		if data.get("entries",[]).is_empty(): empty_state(library,"保存した作品がありません","ワールドのアイテムを保存するか、ワークショップから取得してください。")
		update_columns.call_deferred()
	)

func refresh_workshop() -> void:
	if current_user.is_empty(): return
	workshop_generation+=1
	var generation := workshop_generation
	call_service("workshop",{"offset":workshop_offset,"query":workshop_query.text,"mine":mine_only.button_pressed},func(data):
		if generation!=workshop_generation: return
		clear_list(workshop)
		workshop_total=int(data.get("total",0))
		workshop_page.text=page_label(workshop_offset,12,workshop_total)
		for record in data.get("entries",[]): item_card(workshop,record,"workshop")
		if data.get("entries",[]).is_empty(): empty_state(workshop,"作品が見つかりません","マイライブラリから作品を公開できます。検索中なら、検索語を変えてください。")
		update_columns.call_deferred()
	)

func page_label(offset: int, count: int, total: int) -> String:
	return "0 件" if total==0 else "%d–%d / %d 件" % [offset+1,mini(offset+count,total),total]

func preview_node(parent: Node, data, kind: String, height: int) -> void:
	var area := PanelContainer.new()
	area.custom_minimum_size.y=height
	area.add_theme_stylebox_override("panel",box(Color("eaeef5"),10,Color.TRANSPARENT,0))
	area.clip_contents=true
	parent.add_child(area)
	if data is String and data.begins_with("data:image/png;base64,"):
		var image := Image.new()
		if image.load_png_from_buffer(Marshalls.base64_to_raw(data.get_slice(",",1)))==OK:
			var picture := TextureRect.new()
			picture.texture=ImageTexture.create_from_image(image)
			picture.expand_mode=TextureRect.EXPAND_IGNORE_SIZE
			picture.stretch_mode=TextureRect.STRETCH_KEEP_ASPECT_COVERED
			area.add_child(picture)
			return
	var art := Art.new()
	art.kind=kind
	area.add_child(art)

func item_card(parent: GridContainer, record: Dictionary, kind: String) -> void:
	var card := surface(parent,PAPER,10)
	card.custom_minimum_size.x=174
	card.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	var contents := column(card,7)
	preview_node(contents,record.get("thumbnail"),str(record.get("kind","cube")),110)
	var title := text(str(record.get("title",record.get("name","Untitled"))),15,INK,true)
	title.text_overrun_behavior=TextServer.OVERRUN_TRIM_ELLIPSIS
	title.tooltip_text=title.text
	contents.add_child(title)
	var by := text("by "+str(record.get("publisher_name",record.get("creator_name",""))),12,MUTED)
	by.text_overrun_behavior=TextServer.OVERRUN_TRIM_ELLIPSIS
	contents.add_child(by)
	var note := ""
	if kind=="workshop": note="取得済み" if record.get("in_library",false) else "公開中"
	else: note="公開中" if record.get("publication")!=null else "保存済み"
	contents.add_child(text(note,11,BLUE))
	button(contents,"詳細を見る",func(): select_item(record,kind))

func show_detail_empty() -> void:
	if not is_instance_valid(detail_box): return
	clear_list(detail_box)
	detail_box.add_child(text("作品の詳細",18,INK,true))
	detail_box.add_child(paragraph("作品を選ぶと、内容を確認できます。",13))

func select_item(record: Dictionary, kind: String) -> void:
	selected=record
	selection_kind=kind
	detail_generation+=1
	var generation := detail_generation
	clear_list(detail_box)
	detail_box.add_child(text("読み込み中…",14,MUTED))
	call_service("version" if kind=="library" else "workshop_detail",{"version_id":record.get("version_id",""),"publication_id":record.get("id","")},func(data):
		if generation==detail_generation: render_detail(data,record,kind)
	,func(message):
		if generation==detail_generation: clear_list(detail_box); detail_box.add_child(paragraph(message,13))
	)

func render_detail(data: Dictionary, record: Dictionary, kind: String) -> void:
	clear_list(detail_box)
	preview_node(detail_box,data.get("thumbnail"),str(data.get("kind",record.get("kind","cube"))),158)
	var title: String=str(record.get("name",data.get("title",data.get("name","作品"))))
	detail_box.add_child(paragraph(title,21,INK))
	var creator: String=str(data.get("creator_name",record.get("creator_name","")))
	detail_box.add_child(paragraph("作者  "+creator,13))
	if kind=="workshop" and data.get("publisher_name",creator)!=creator: detail_box.add_child(paragraph("公開者  "+str(data.publisher_name),12))
	var type_name := kind_label(str(record.get("kind",data.get("kind","cube"))))
	var scripts := int(data.get("script_count",data.get("object",{}).get("scripts",[]).size()))
	detail_box.add_child(text(type_name+"  ·  ロジック %d 件" % scripts,12,MUTED))
	if kind=="workshop" and not str(data.get("description","")).is_empty(): detail_box.add_child(paragraph(data.description,13,INK))
	rule(detail_box)
	if kind=="workshop":
		var in_library: bool=data.get("in_library",false)
		var action: String="ライブラリに追加済み" if in_library else "ライブラリに保存"
		var acquire_button := button(detail_box,action,func():
			call_service("acquire_item",{"publication_id":data.id},func(result):
				status.text="ライブラリに保存しました。"
				select_item(result.publication,"workshop")
				refresh_workshop()
			)
		,true)
		acquire_button.disabled=in_library
		if in_library:
			button(detail_box,"マイライブラリを開く",func(): switch_page("library"))
		if data.get("can_manage",false): button(detail_box,"公開を停止…",func(): confirm_unpublish(data))
	else:
		detail_box.add_child(text("保存  "+Time.get_datetime_string_from_unix_time(int(data.saved_at)).replace("T"," "),11,MUTED))
		var place := button(detail_box,"ワールドに配置",func():
			var p: Vector3=app.player_position+Vector3(0,1,-2)
			call_service("place_item",{"version_id":record.version_id,"position":[p.x,p.y,p.z]},func(_v): shade.hide())
		,true)
		place.disabled=current_session.is_empty()
		place.tooltip_text="セッションに参加すると配置できます。" if place.disabled else ""
		if data.get("publication") is Dictionary:
			button(detail_box,"公開中の作品を見る",func(): switch_page("workshop"); select_item(data.publication,"workshop"))
		else:
			button(detail_box,"ワークショップに公開…",func(): show_publish(record))
		button(detail_box,"名前を変更…",func(): rename_entry(record))
		button(detail_box,"ユーザーに直接渡す…",func(): share_entry(record))
		button(detail_box,"ライブラリから外す…",func(): confirm_remove(record))

func kind_label(kind: String) -> String:
	return {"cube":"オブジェクト","model":"3Dモデル","image":"画像","video":"動画"}.get(kind,"アイテム")

func modal(title: String, ok: String, width: int = 470) -> ConfirmationDialog:
	var dialog := ConfirmationDialog.new()
	dialog.title=title
	dialog.dialog_text=""
	dialog.ok_button_text=ok
	dialog.cancel_button_text="キャンセル"
	dialog.theme=shell.theme
	dialog.min_size=Vector2i(width,0)
	dialog.unresizable=true
	app.add_child(dialog)
	dialog.canceled.connect(dialog.queue_free)
	return dialog

func show_publish(record: Dictionary) -> void:
	publish_dialog=modal("ワークショップに公開","公開する",490)
	publish_dialog.dialog_hide_on_ok=false
	publishing_version=record.version_id
	var dialog := publish_dialog
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size=Vector2(450,mini(300,int(app.get_viewport().get_visible_rect().size.y)-200))
	scroll.horizontal_scroll_mode=ScrollContainer.SCROLL_MODE_DISABLED
	publish_dialog.add_child(scroll)
	var form := column(scroll,12)
	form.custom_minimum_size.x=430
	form.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	form.add_child(paragraph("公開するのは、この保存版です。ワールドで後から編集しても、公開中の内容は変わりません。",13))
	form.add_child(text("作品名",13,INK,true))
	publish_title=field(form,"作品名")
	publish_title.text=record.name
	publish_title.max_length=80
	form.add_child(text("説明",13,INK,true))
	publish_description=TextEdit.new()
	publish_description.placeholder_text="どんな作品か、使い方などを紹介してください。"
	publish_description.custom_minimum_size=Vector2(430,110)
	publish_description.wrap_mode=TextEdit.LINE_WRAPPING_BOUNDARY
	form.add_child(publish_description)
	publish_error=paragraph("",12,Color("a8321f")); form.add_child(publish_error)
	publish_dialog.confirmed.connect(func():
		if publish_title.text.strip_edges().is_empty(): publish_error.text="作品名を入力してください。"; return
		if publish_description.text.length()>2000: publish_error.text="説明は2000文字以内にしてください。"; return
		publish_dialog.get_ok_button().disabled=true
		call_service("publish_item",{"version_id":publishing_version,"title":publish_title.text,"description":publish_description.text},func(data):
			if is_instance_valid(dialog): dialog.queue_free()
			switch_page("workshop",false)
			mine_only.button_pressed=true
			refresh_workshop()
			select_item(data,"workshop")
			status.text="ワークショップに公開しました。"
		,func(message):
			if is_instance_valid(dialog): publish_error.text=message; dialog.get_ok_button().disabled=false
		)
	)
	publish_dialog.popup_centered(Vector2i(510,420))

func confirm_unpublish(record: Dictionary) -> void:
	var dialog := modal("公開を停止しますか？","公開を停止")
	dialog.dialog_text="ワークショップでの公開を停止します。保存済みのライブラリや配置物は残ります。"
	dialog.confirmed.connect(func(): call_service("unpublish_item",{"publication_id":record.id},func(_v): selected={}; show_detail_empty(); refresh_workshop(); status.text="公開を停止しました。"); dialog.queue_free())
	dialog.popup_centered()

func confirm_remove(record: Dictionary) -> void:
	var dialog := modal("ライブラリから外しますか？","ライブラリから外す")
	dialog.dialog_text="この一覧から外します。ワールドに配置したものや公開中の作品は残ります。"
	dialog.confirmed.connect(func(): call_service("remove_entry",{"entry_id":record.id},func(_v): selected={}; show_detail_empty(); refresh_library()); dialog.queue_free())
	dialog.popup_centered()

func rename_entry(record: Dictionary) -> void:
	var dialog := modal("ライブラリの名前を変更","変更する")
	var form := column(dialog)
	var input := field(form,"名前")
	input.text=record.name
	input.max_length=80
	dialog.confirmed.connect(func(): call_service("rename_entry",{"entry_id":record.id,"name":input.text},func(_v): show_detail_empty(); refresh_library()); dialog.queue_free())
	dialog.popup_centered()

func share_entry(record: Dictionary) -> void:
	var dialog := modal("ユーザーに直接渡す","渡す")
	var form := column(dialog)
	var recipient := field(form,"相手のユーザーコード")
	dialog.confirmed.connect(func(): call_service("share_item",{"version_id":record.version_id,"user_id":recipient.text.strip_edges()},func(_v): status.text="作品を渡しました。"); dialog.queue_free())
	dialog.popup_centered()

func save_selected() -> void:
	if app.current.is_empty(): status.text="ワールドでアイテムを選択してください。"; return
	var object: Dictionary=app.current.duplicate(true)
	var values := {"target":app.address_of(object),"name":str(app.fields.name.value)}
	save_button.disabled=true
	var thumbnail: PackedByteArray=await capture_item(object)
	if not thumbnail.is_empty(): values.thumbnail="data:image/png;base64,"+Marshalls.raw_to_base64(thumbnail)
	call_service("save_item",values,func(_v): library_offset=0; refresh_library(); save_button.disabled=false; status.text="アイテムを保存しました。",func(_message): save_button.disabled=false)

func capture_item(object: Dictionary) -> PackedByteArray:
	if DisplayServer.get_name()=="headless": return PackedByteArray()
	var id: String=object.address.instance_id
	if not app.render_nodes.has(id): return PackedByteArray()
	var viewport := SubViewport.new()
	viewport.size=Vector2i(240,150)
	viewport.own_world_3d=true
	viewport.render_target_update_mode=SubViewport.UPDATE_ALWAYS
	app.add_child(viewport)
	var clone: Node3D=app.render_nodes[id].duplicate()
	viewport.add_child(clone)
	clone.position=Vector3.ZERO
	var bounds := mesh_bounds(clone)
	var center := bounds.get_center()
	var distance := maxf(bounds.size.length()*1.9,1.0)
	var camera := Camera3D.new()
	viewport.add_child(camera)
	camera.fov=32
	camera.far=maxf(1000,distance*4)
	camera.position=center+Vector3(1.1,0.7,1.5).normalized()*distance
	camera.look_at(center)
	var light := DirectionalLight3D.new()
	light.rotation_degrees=Vector3(-40,-30,0)
	viewport.add_child(light)
	var environment := WorldEnvironment.new()
	environment.environment=Environment.new()
	environment.environment.background_mode=Environment.BG_COLOR
	environment.environment.background_color=Color("eaeef5")
	environment.environment.ambient_light_source=Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color=Color.WHITE
	environment.environment.ambient_light_energy=0.8
	viewport.add_child(environment)
	await app.get_tree().process_frame
	await RenderingServer.frame_post_draw
	var image: Image=viewport.get_texture().get_image()
	viewport.queue_free()
	if image==null or image.is_empty(): return PackedByteArray()
	var bytes := image.save_png_to_buffer()
	if bytes.size()>48000: image.resize(160,100,Image.INTERPOLATE_LANCZOS); bytes=image.save_png_to_buffer()
	return bytes if bytes.size()<=48000 else PackedByteArray()

func mesh_bounds(node: Node3D) -> AABB:
	var result := AABB()
	if node is MeshInstance3D and node.mesh!=null and node.is_visible_in_tree(): result=node.global_transform*node.get_aabb()
	for child in node.get_children():
		if child is Node3D:
			var child_bounds := mesh_bounds(child)
			if child_bounds.size.length()>0: result=child_bounds if result.size.length()==0 else result.merge(child_bounds)
	return result

func choose_world() -> void:
	var dialog := FileDialog.new()
	dialog.use_native_dialog=true
	dialog.access=FileDialog.ACCESS_FILESYSTEM
	dialog.file_mode=FileDialog.FILE_MODE_OPEN_FILE
	dialog.filters=PackedStringArray(["*.json ; Saved Kokoverse world"])
	app.add_child(dialog)
	dialog.file_selected.connect(func(path): load_world=path; world_choice.text="開始ワールド: "+path.get_file(); dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free)
	dialog.popup_centered_ratio(0.7)
