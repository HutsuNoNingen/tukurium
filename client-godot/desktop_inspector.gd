extends "res://inspector_view.gd"
## Desktop layout of the same inspector model and editing operations.
## No SubViewport texture or world-panel dimensions are used here.
var active_tab := 0
var pending_tab := -1
var local_script := ""

func build_layout() -> void:
	var margin := MarginContainer.new()
	for edge in ["left", "top", "right", "bottom"]:
		margin.add_theme_constant_override("margin_" + edge, 12)
	add_child(margin)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 10)
	margin.add_child(column)
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 8)
	column.add_child(header)
	title.text = "Inspector"
	title.add_theme_font_size_override("font_size", 20)
	header.add_child(title)
	target_picker.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	target_picker.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	header.add_child(target_picker)
	add_button(header, "Delete shared panel", func(): manager.close_panel(panel_id)).tooltip_text = "Delete this inspector for everyone. The Windows close button only closes this window."
	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 8)
	column.add_child(actions)
	build_actions(actions)
	column.add_child(HSeparator.new())
	var body := HSplitContainer.new()
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	body.split_offset = 245
	column.add_child(body)
	build_body(body)
	script_view.local_selection = true
	hierarchy.custom_minimum_size.x = 200
	hierarchy.tree.add_theme_font_size_override("font_size", 17)
	column.add_child(HSeparator.new())
	column.add_child(message)

func select_tab(index: int) -> void:
	finish_all()
	pending_tab = index

func tick() -> void:
	super.tick()
	if pending_tab >= 0 and edits.is_empty() and not script_view.busy():
		active_tab = pending_tab
		pending_tab = -1
		refresh_pages(panel_state)

func show_requested_script(id: String) -> void:
	local_script = "" if id == "*" else id
	active_tab = 1
	pending_tab = -1
	refresh_pages(panel_state)

func refresh_pages(_object: Dictionary) -> void:
	tabs.set_block_signals(true)
	tabs.current_tab = active_tab
	tabs.set_block_signals(false)
	scroll.visible = active_tab == 0
	script_mount.visible = active_tab == 1
	script_view.active = active_tab == 1
	if script_view.active:
		script_view.refresh(target_id, null if local_script.is_empty() else local_script)
		if script_view.editor.selected == local_script: local_script = ""

func restore_scroll(_object: Dictionary) -> void:
	pass # Desktop scrolling is local and independent of the world surface.

func on_scroll(_value: float) -> void:
	pass # Resizing or scrolling this view must never write panel_scroll.
