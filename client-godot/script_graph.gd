extends VBoxContainer
## Visual source editor. The graph JSON is stored in ScriptComponent.source.
signal changed
signal operations(ops: Array)
const PREFIX := "@tukurium-graph-v1\n"
const PORT_HELP := "Port colors and labels show types · Right-click empty space to add nodes."
const TYPES := {
	"on_update":[[], [["flow","flow"]]], "on_interact":[[], [["flow","flow"]]], "on_button_press":[[], [["flow","flow"]]], "on_button_release":[[], [["flow","flow"]]], "on_contact_start":[[], [["flow","flow"]]], "on_contact_end":[[], [["flow","flow"]]], "on_drive":[[], [["flow","flow"]]],
	"branch":[[["flow","flow"],["test","bool"]],[["yes","flow"],["no","flow"]]],
	"sequence":[[["flow","flow"]],[["first","flow"],["second","flow"]]],
	"write_field":[[["flow","flow"],["value","any"]],[["next","flow"]]],
	"drive_field":[[["flow","flow"],["value","any"]],[["next","flow"]]],
	"set_state":[[["flow","flow"],["value","any"]],[["next","flow"]]],
	"capture_camera":[[["flow","flow"]],[["next","flow"]]],
	"for_range":[[["flow","flow"],["start","number"],["end","number"],["step","number"]],[["body","flow"],["done","flow"],["index","number"]]],
	"while_loop":[[["flow","flow"],["test","bool"]],[["body","flow"],["done","flow"]]],
	"break_loop":[[["flow","flow"]],[]],
	"continue_loop":[[["flow","flow"]],[]],
	"number":[[],[["value","number"]]], "public_number":[[],[["value","number"]]],
	"bool":[[],[["value","bool"]]], "string":[[],[["value","string"]]],
	"vec3":[[],[["value","vec3"]]], "color":[[],[["value","color"]]],
	"delta":[[],[["value","number"]]], "time":[[],[["value","number"]]], "world_time":[[],[["value","number"]]], "contact_other_id":[[],[["value","string"]]],
	"read_field":[[],[["value","any"]]], "get_state":[[],[["value","any"]]],
	"display":[[["value","any"]],[["value","any"]]],
	"add":[[["a","number"],["b","number"]],[["value","number"]]],
	"subtract":[[["a","number"],["b","number"]],[["value","number"]]],
	"multiply":[[["a","number"],["b","number"]],[["value","number"]]],
	"divide":[[["a","number"],["b","number"]],[["value","number"]]],
	"modulo":[[["a","number"],["b","number"]],[["value","number"]]],
	"min":[[["a","number"],["b","number"]],[["value","number"]]],
	"max":[[["a","number"],["b","number"]],[["value","number"]]],
	"less":[[["a","number"],["b","number"]],[["value","bool"]]],
	"greater":[[["a","number"],["b","number"]],[["value","bool"]]],
	"equal":[[["a","any"],["b","any"]],[["value","bool"]]],
	"and":[[["a","bool"],["b","bool"]],[["value","bool"]]],
	"or":[[["a","bool"],["b","bool"]],[["value","bool"]]],
	"not":[[["a","bool"]],[["value","bool"]]],
	"ceil":[[["value","number"]],[["value","number"]]],
	"abs":[[["value","number"]],[["value","number"]]],
	"floor":[[["value","number"]],[["value","number"]]],
	"sin":[[["value","number"]],[["value","number"]]],
	"cos":[[["value","number"]],[["value","number"]]],
	"clamp":[[["value","number"],["min","number"],["max","number"]],[["value","number"]]],
	"lerp":[[["a","number"],["b","number"],["t","number"]],[["value","number"]]],
	"vec3_make":[[["x","number"],["y","number"],["z","number"]],[["value","vec3"]]],
	"vec3_split":[[["value","vec3"]],[["x","number"],["y","number"],["z","number"]]],
	"vec3_add":[[["a","vec3"],["b","vec3"]],[["value","vec3"]]],
	"vec3_subtract":[[["a","vec3"],["b","vec3"]],[["value","vec3"]]],
	"vec3_scale":[[["value","vec3"],["factor","number"]],[["value","vec3"]]],
	"vec3_dot":[[["a","vec3"],["b","vec3"]],[["value","number"]]],
	"vec3_cross":[[["a","vec3"],["b","vec3"]],[["value","vec3"]]],
	"vec3_length":[[["value","vec3"]],[["value","number"]]],
	"vec3_normalize":[[["value","vec3"]],[["value","vec3"]]],
	"vec3_distance":[[["a","vec3"],["b","vec3"]],[["value","number"]]],
	"vec3_lerp":[[["a","vec3"],["b","vec3"],["t","number"]],[["value","vec3"]]],
	"negate":[[["value","number"]],[["value","number"]]],
	"sqrt":[[["value","number"]],[["value","number"]]],
	"power":[[["a","number"],["b","number"]],[["value","number"]]],
	"round":[[["value","number"]],[["value","number"]]],
	"sign":[[["value","number"]],[["value","number"]]],
	"tan":[[["value","number"]],[["value","number"]]],
	"asin":[[["value","number"]],[["value","number"]]],
	"acos":[[["value","number"]],[["value","number"]]],
	"atan2":[[["y","number"],["x","number"]],[["value","number"]]],
	"log":[[["value","number"]],[["value","number"]]],
	"exp":[[["value","number"]],[["value","number"]]],
	"remap":[[["value","number"],["in_min","number"],["in_max","number"],["out_min","number"],["out_max","number"]],[["value","number"]]],
	"smoothstep":[[["edge0","number"],["edge1","number"],["value","number"]],[["value","number"]]],
	"ping_pong":[[["value","number"],["length","number"]],[["value","number"]]],
	"less_equal":[[["a","number"],["b","number"]],[["value","bool"]]],
	"greater_equal":[[["a","number"],["b","number"]],[["value","bool"]]],
	"not_equal":[[["a","any"],["b","any"]],[["value","bool"]]],
	"xor":[[["a","bool"],["b","bool"]],[["value","bool"]]],
	"parse_number":[[["text","string"]],[["value","number"]]],
	"number_to_bool":[[["value","number"]],[["value","bool"]]],
	"bool_to_number":[[["value","bool"]],[["value","number"]]],
	"string_length":[[["text","string"]],[["value","number"]]],
	"string_contains":[[["text","string"],["part","string"]],[["value","bool"]]],
	"string_starts_with":[[["text","string"],["part","string"]],[["value","bool"]]],
	"string_ends_with":[[["text","string"],["part","string"]],[["value","bool"]]],
	"string_slice":[[["text","string"],["start","number"],["count","number"]],[["value","string"]]],
	"vec3_angle":[[["a","vec3"],["b","vec3"]],[["value","number"]]],
	"vec3_reflect":[[["value","vec3"],["normal","vec3"]],[["value","vec3"]]],
	"vec3_move_towards":[[["a","vec3"],["b","vec3"],["max_delta","number"]],[["value","vec3"]]],
	"color_make":[[["r","number"],["g","number"],["b","number"],["a","number"]],[["value","color"]]],
	"color_split":[[["value","color"]],[["r","number"],["g","number"],["b","number"],["a","number"]]],
	"color_lerp":[[["a","color"],["b","color"],["t","number"]],[["value","color"]]],
	"select":[[["test","bool"],["yes","any"],["no","any"]],[["value","any"]]],
	"to_string":[[["value","any"]],[["value","string"]]],
	"join":[[["a","string"],["b","string"]],[["value","string"]]],
	"rotation_y":[[["degrees","number"],["base","any"]],[["value","any"]]]
}
const COLORS := {"flow":Color("f0b86b"),"number":Color("7bd8cb"),"bool":Color("b5a6ff"),"string":Color("e7aeb6"),"vec3":Color("9cc7fa"),"color":Color("f0d891"),"rotation":Color("a4b8ff"),"any":Color("cbd5dc")}
const TYPE_NAMES := {"flow":"FLOW","number":"NUMBER","bool":"BOOL","string":"TEXT","vec3":"VECTOR 3","color":"COLOR","rotation":"ROTATION","any":"ANY"}
const PORT_IDS := {"flow":0,"number":1,"bool":2,"string":3,"vec3":4,"color":5,"any":6}
const MATH_HINTS := {
	"abs":"Absolute value of a finite number.",
	"floor":"Round down to the nearest integer.",
	"sin":"Sine of an angle in radians. Use Time × speed for oscillation.",
	"cos":"Cosine of an angle in radians. Pair with Sine for circular motion.",
	"clamp":"Keep Value between Min and Max, including both boundaries. Min must not exceed Max.",
	"lerp":"Blend A to B. T is clamped to 0–1; endpoints are exact.",
	"vec3_make":"Build a Vector 3 from calculated X, Y and Z numbers.",
	"vec3_split":"Read the X, Y and Z numbers of a Vector 3 from separate outputs.",
	"vec3_add":"Add two Vector 3 values, component by component.",
	"vec3_subtract":"Subtract B from A. Target minus current position gives a direction.",
	"vec3_scale":"Multiply each component by Factor. A negative factor reverses direction.",
	"vec3_dot":"Dot product: compare directions or project one vector onto another.",
	"vec3_cross":"Cross product: a vector perpendicular to A and B. Input order matters.",
	"vec3_length":"Euclidean length of a Vector 3.",
	"vec3_normalize":"Keep direction with length 1. A zero vector returns a zero vector.",
	"vec3_distance":"Euclidean distance between points A and B.",
	"vec3_lerp":"Blend Vector 3 A to B. T is clamped to 0–1; endpoints are exact.",
	"negate":"Reverse the sign of a finite number.",
	"sqrt":"Square root. The input must be zero or positive.",
	"power":"Raise A to the power B. The result must be finite and real.",
	"round":"Round to the nearest integer; halves round away from zero.",
	"sign":"Return -1, 0 or 1 according to the sign of a number.",
	"tan":"Tangent of an angle in radians.",
	"asin":"Inverse sine in radians. Input must be between -1 and 1.",
	"acos":"Inverse cosine in radians. Input must be between -1 and 1.",
	"atan2":"Direction angle in radians from Y and X.",
	"log":"Natural logarithm. Input must be greater than zero.",
	"exp":"Exponential e raised to the input power. The result must be finite.",
	"remap":"Map a number from one range into another, without clamping. Source endpoints must differ.",
	"smoothstep":"A smooth 0–1 transition from Edge0 to Edge1. Edges must be increasing.",
	"ping_pong":"Repeat a triangular motion from 0 to Length and back. Length must be positive.",
	"less_equal":"True when A is less than or equal to B.",
	"greater_equal":"True when A is greater than or equal to B.",
	"equal":"Compare scalar values or numeric arrays with 2–4 components. Numbers compare by value.",
	"not_equal":"Compare scalar values or numeric arrays with 2–4 components. Different types are unequal.",
	"xor":"True when exactly one of A or B is true.",
	"parse_number":"Convert trimmed decimal text into a finite number. Invalid text is unavailable.",
	"number_to_bool":"Zero becomes false; every other finite number becomes true.",
	"bool_to_number":"False becomes 0; true becomes 1.",
	"string_length":"Count Unicode characters, including each emoji as one character. Text is limited to 512 UTF-8 bytes.",
	"string_contains":"Check whether Text contains Part. Comparison is case-sensitive.",
	"string_starts_with":"Check whether Text starts with Part. Comparison is case-sensitive.",
	"string_ends_with":"Check whether Text ends with Part. Comparison is case-sensitive.",
	"string_slice":"Read Count Unicode characters from zero-based Start. Start and Count must be nonnegative integers.",
	"vec3_angle":"Angle between A and B in radians. Neither vector may be zero.",
	"vec3_reflect":"Reflect Value across Normal. Normal is normalized and must not be zero.",
	"vec3_move_towards":"Move A toward B by at most Max Delta. Max Delta must be nonnegative.",
	"color_make":"Build RGBA color channels. Each channel must be finite and between 0 and 1.",
	"color_split":"Read R, G, B and A from separate NUMBER outputs.",
	"color_lerp":"Blend RGBA colors. T is clamped to 0–1.",
	"branch":"If / Else: run Yes when Test is true; otherwise run No.",
	"for_range":"Run Body from Start through End with a nonzero integer Step, then Done.\nIndex is available only inside Body.\nAll loops share a 64-iteration callback limit.",
	"while_loop":"Repeat Body while Test is true, then Done.\nTest is checked before each iteration.\nAll loops share a 64-iteration callback limit.",
	"break_loop":"Leave the nearest loop immediately. Connect only inside a loop Body.",
	"continue_loop":"Skip to the next iteration of the nearest loop. Connect only inside a loop Body."
}
var data := {"name":"Graph1","nodes":[],"links":[]}
var canvas: GraphEdit
var graph_area: Control
var palette: OptionButton
var component_name: LineEdit
var message: Label
var context_panel: PanelContainer
var context_search: LineEdit
var context_buttons: Dictionary = {}
var context_position := Vector2.ZERO
var display_labels: Dictionary = {}
var app
var target_id := ""
var script_id := ""
var next_preview := 0
var loading := false
var collaborative_ids := false
var observed: Dictionary = {}

func node_label(kind: String) -> String:
	if kind == "branch": return "If / Else (Branch)"
	if kind == "sin": return "Sine (radians)"
	if kind == "cos": return "Cosine (radians)"
	if kind == "tan": return "Tangent (radians)"
	if kind == "asin": return "Arc Sine (radians)"
	if kind == "acos": return "Arc Cosine (radians)"
	if kind == "atan2": return "Atan2 (radians)"
	if kind == "vec3_make": return "Make Vector 3"
	if kind == "vec3_split": return "Split Vector 3"
	if kind.begins_with("vec3_"): return "Vector " + kind.trim_prefix("vec3_").capitalize()
	return kind.replace("_", " ").capitalize()

func node_category(kind: String) -> String:
	if kind in ["for_range","while_loop","break_loop","continue_loop"]: return "Flow / Loops"
	if kind == "branch": return "Flow / Conditions"
	if kind in ["less","greater","equal","less_equal","greater_equal","not_equal","xor","and","or","not"]: return "Math / Logic"
	if kind in ["parse_number","number_to_bool","bool_to_number"]: return "Convert"
	if kind.begins_with("string_"): return "Text"
	if kind.begins_with("color_"): return "Color"
	if kind.begins_with("vec3_"): return "Math / Vector"
	if MATH_HINTS.has(kind): return "Math / Scalar"
	if kind.begins_with("on_"): return "Event"
	if kind in ["branch","sequence","write_field","drive_field","set_state","capture_camera"]: return "Flow"
	if kind in ["number","public_number","bool","string","vec3","color","delta","time","world_time","contact_other_id","read_field","get_state"]: return "Value"
	return "Convert" if kind in ["to_string","join","rotation_y"] else "Math / Logic"

func _ready() -> void:
	size_flags_vertical = Control.SIZE_EXPAND_FILL
	var tools := HBoxContainer.new()
	add_child(tools)
	var heading := Label.new()
	heading.text = "Component"
	tools.add_child(heading)
	component_name = LineEdit.new()
	component_name.custom_minimum_size.x = 135
	component_name.max_length = 40
	component_name.text_changed.connect(func(value): data.name = value; touch())
	tools.add_child(component_name)
	palette = OptionButton.new()
	palette.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for kind in TYPES:
		palette.add_item(node_category(kind) + " · " + node_label(kind))
		palette.set_item_metadata(palette.item_count - 1, palette.get_item_text(palette.item_count - 1).split(" · ", true, 1))
		palette.get_popup().set_item_tooltip(palette.item_count - 1, MATH_HINTS.get(kind, ""))
	tools.add_child(palette)
	var add := Button.new()
	add.text = "+ Node"
	add.pressed.connect(func(): add_node(TYPES.keys()[palette.selected]))
	tools.add_child(add)
	message = Label.new()
	message.text = PORT_HELP
	message.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	add_child(message)
	graph_area = Control.new()
	graph_area.size_flags_vertical = Control.SIZE_EXPAND_FILL
	graph_area.custom_minimum_size.y = 160
	add_child(graph_area)
	canvas = GraphEdit.new()
	canvas.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	canvas.minimap_enabled = true
	canvas.minimap_size = Vector2(120,80)
	canvas.show_arrange_button = false
	canvas.right_disconnects = true
	for index in range(1,7):
		canvas.add_valid_connection_type(6,index)
		canvas.add_valid_connection_type(index,6)
	canvas.connection_request.connect(connect_ports)
	canvas.disconnection_request.connect(disconnect_ports)
	canvas.delete_nodes_request.connect(delete_nodes)
	canvas.end_node_move.connect(func(): capture_layout(); touch())
	canvas.popup_request.connect(open_context_menu)
	canvas.gui_input.connect(func(event):
		if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
			context_panel.hide())
	graph_area.add_child(canvas)
	canvas.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	context_panel = PanelContainer.new()
	context_panel.custom_minimum_size.x = 250
	context_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	context_panel.z_index = 10
	var context_style := StyleBoxFlat.new()
	context_style.bg_color = Color("17232f")
	context_style.border_color = Color("517986")
	context_style.set_border_width_all(1)
	context_style.set_corner_radius_all(6)
	context_style.set_content_margin_all(6)
	context_panel.add_theme_stylebox_override("panel",context_style)
	graph_area.add_child(context_panel)
	var context_body := VBoxContainer.new()
	context_panel.add_child(context_body)
	context_search = LineEdit.new()
	context_search.placeholder_text = "Search nodes"
	context_search.text_changed.connect(filter_context_menu)
	context_search.gui_input.connect(func(event):
		if event is InputEventKey and event.pressed and event.keycode == KEY_ESCAPE:
			context_panel.hide())
	context_body.add_child(context_search)
	var menu_scroll := ScrollContainer.new()
	menu_scroll.custom_minimum_size = Vector2(230,220)
	context_body.add_child(menu_scroll)
	var menu_list := VBoxContainer.new()
	menu_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	menu_scroll.add_child(menu_list)
	for kind in TYPES:
		var choice := Button.new()
		choice.text = node_label(kind)
		choice.tooltip_text = MATH_HINTS.get(kind, "")
		choice.alignment = HORIZONTAL_ALIGNMENT_LEFT
		choice.focus_mode = Control.FOCUS_NONE
		choice.pressed.connect(func():
			add_node(kind, context_position)
			context_panel.hide())
		menu_list.add_child(choice)
		context_buttons[kind] = choice
	context_panel.hide()
	AppLanguage.language_changed.connect(refresh_language)
	refresh_language()

func refresh_language() -> void:
	# Keep identifiers and authored graph values separate from display captions.
	for index in palette.item_count:
		var sources: PackedStringArray = palette.get_item_metadata(index)
		palette.set_item_text(index, tr(sources[0]) + " · " + tr(sources[1]))
	for child in canvas.get_children():
		if child is GraphNode and child.has_meta("title_source"):
			child.title = tr(str(child.get_meta("title_source"))) + "  ·  " + str(child.name)
	filter_context_menu(context_search.text)
	refresh_port_visuals()

func open_context_menu(at_position: Vector2) -> void:
	context_position = (at_position + canvas.scroll_offset) / canvas.zoom
	context_search.clear()
	filter_context_menu("")
	context_panel.show()
	var size_hint := context_panel.get_combined_minimum_size()
	context_panel.position = Vector2(
		clampf(at_position.x, 0.0, maxf(0.0, graph_area.size.x - size_hint.x)),
		clampf(at_position.y, 0.0, maxf(0.0, graph_area.size.y - size_hint.y)))
	context_search.grab_focus()

func filter_context_menu(query: String) -> void:
	for kind in context_buttons:
		var search_text := "%s %s %s %s" % [kind, node_label(kind), node_category(kind), MATH_HINTS.get(kind, "")]
		context_buttons[kind].visible = query.is_empty() or search_text.containsn(query) or tr(str(context_buttons[kind].text)).containsn(query)

func capture_layout() -> void:
	for item in data.nodes:
		var view := canvas.get_node_or_null(str(item.id)) as GraphNode
		if view != null:
			item.x = view.position_offset.x
			item.y = view.position_offset.y

func fresh(name: String) -> void:
	data = {"name":name,"nodes":[{"id":"n1","kind":"on_update","x":40.0,"y":80.0}],"links":[]}
	redraw()
	observed = data.duplicate(true)

func load_source(source: String) -> void:
	if not source.begins_with(PREFIX): return
	var parsed = JSON.parse_string(source.substr(PREFIX.length()))
	if parsed is Dictionary and parsed.has("nodes") and parsed.has("links"):
		data = parsed.duplicate(true)
		redraw()
		observed = data.duplicate(true)

func load_data(value: Dictionary) -> void:
	var focused := ""
	var caret := 0
	var owner := get_viewport().gui_get_focus_owner() if is_inside_tree() else null
	if owner is LineEdit and owner != component_name:
		var parent := owner.get_parent()
		if parent is GraphNode:
			focused = parent.name
			caret = owner.caret_column
	data = value.duplicate(true)
	redraw()
	observed = data.duplicate(true)
	if not focused.is_empty():
		var node := canvas.get_node_or_null(focused)
		if node != null:
			var inputs := node.find_children("*", "LineEdit", true, false)
			if not inputs.is_empty():
				inputs[0].grab_focus.call_deferred()
				inputs[0].set_caret_column.call_deferred(caret)

func source() -> String:
	capture_layout()
	return PREFIX + JSON.stringify(data)

func touch() -> void:
	if loading: return
	var ops := difference(observed, data)
	observed = data.duplicate(true)
	changed.emit()
	if not ops.is_empty(): operations.emit(ops)

func difference(before: Dictionary, after: Dictionary) -> Array:
	var ops: Array = []
	if before.get("name") != after.get("name"): ops.append({"kind":"name","value":after.name})
	var old_nodes := {}
	var new_nodes := {}
	for node in before.get("nodes",[]): old_nodes[str(node.id)] = node
	for node in after.get("nodes",[]): new_nodes[str(node.id)] = node
	for id in old_nodes:
		if not new_nodes.has(id): ops.append({"kind":"remove_node","id":id})
	for id in new_nodes:
		if not old_nodes.has(id):
			ops.append({"kind":"add_node","node":new_nodes[id].duplicate(true)})
		else:
			var old: Dictionary = old_nodes[id]
			var node: Dictionary = new_nodes[id]
			if old.get("x") != node.get("x") or old.get("y") != node.get("y"):
				ops.append({"kind":"position","id":id,"x":node.x,"y":node.y})
			if old.get("value") != node.get("value"):
				ops.append({"kind":"value","id":id,"value":node.get("value")})
	for link in before.get("links",[]):
		if not after.get("links",[]).has(link) and new_nodes.has(str(link.from)) and new_nodes.has(str(link.to)):
			ops.append({"kind":"disconnect","link":link.duplicate(true)})
	for link in after.get("links",[]):
		if not before.get("links",[]).has(link): ops.append({"kind":"connect","link":link.duplicate(true)})
	return ops

func project(base: Dictionary, changes: Array) -> Dictionary:
	var result := base.duplicate(true)
	for op in changes:
		match str(op.kind):
			"name": result.name = op.value
			"add_node":
				if not result.nodes.any(func(n): return n.id == op.node.id): result.nodes.append(op.node.duplicate(true))
			"remove_node":
				result.nodes = result.nodes.filter(func(n): return n.id != op.id)
				result.links = result.links.filter(func(l): return l.from != op.id and l.to != op.id)
			"position", "value":
				for node in result.nodes:
					if node.id != op.id: continue
					if op.kind == "position": node.x = op.x; node.y = op.y
					else: node.value = op.value
			"connect":
				result.links = result.links.filter(func(l): return l.to != op.link.to or l.input != op.link.input)
				result.links.append(op.link.duplicate(true))
			"disconnect": result.links.erase(op.link)
	return result

func bound_field_type(node_id: String) -> String:
	if app == null or target_id.is_empty() or script_id.is_empty(): return "any"
	for component in app.objects.get(target_id, {}).get("scripts", []):
		if str(component.id) != script_id: continue
		var reference: Variant = component.get("parameters", {}).get("ref_" + node_id)
		if not reference is Dictionary or reference.get("kind") != "field" or not reference.get("target") is Dictionary: return "any"
		var address: Dictionary = reference.target
		for field in app.objects.get(str(address.get("instance_id", "")), {}).get("fields", []):
			if str(field.id) != str(reference.get("field_id", "")) or int(field.component_id) != int(address.get("local_id", -1)): continue
			var value_type := str(field.value_type)
			if value_type == "quat": return "rotation"
			return value_type if COLORS.has(value_type) else "any"
	return "any"

func linked_port_type(item: Dictionary, input_name: String, seen: Dictionary) -> String:
	for link in data.links:
		if link.to != item.id or link.input != input_name: continue
		var source_item := find_item(str(link.from))
		if source_item.is_empty() or not TYPES.has(str(source_item.kind)): return "any"
		for output in TYPES[source_item.kind][1]:
			if output[0] == link.out:
				return shown_port_type(source_item, "out", str(output[0]), str(output[1]), seen)
	return "any"

func shown_port_type(item: Dictionary, side: String, port_name: String, declared_type: String, seen: Dictionary = {}) -> String:
	if declared_type != "any": return declared_type
	var key := "%s/%s/%s" % [item.id, side, port_name]
	if seen.has(key): return "any"
	var visited := seen.duplicate()
	visited[key] = true
	if item.kind == "read_field" and side == "out": return bound_field_type(str(item.id))
	if item.kind in ["write_field", "drive_field"] and side == "in" and port_name == "value": return bound_field_type(str(item.id))
	if item.kind == "rotation_y" and (side == "out" or port_name == "base"): return "rotation"
	if item.kind == "select":
		if side == "out" and port_name == "value":
			var yes := linked_port_type(item, "yes", visited)
			var no := linked_port_type(item, "no", visited)
			if yes == no or no == "any": return yes
			return no if yes == "any" else "any"
		if side == "in" and port_name in ["yes", "no"]:
			var own := linked_port_type(item, port_name, visited)
			return own if own != "any" else linked_port_type(item, "no" if port_name == "yes" else "yes", visited)
	if side == "in": return linked_port_type(item, port_name, visited)
	if item.kind == "display" and side == "out": return linked_port_type(item, "value", visited)
	return "any"

func connection_input_type(item: Dictionary, port_name: String, declared_type: String) -> String:
	if declared_type != "any": return declared_type
	if item.kind == "select" and port_name in ["yes", "no"]:
		return linked_port_type(item, "no" if port_name == "yes" else "yes", {})
	if item.kind in ["write_field", "drive_field", "rotation_y"]:
		return shown_port_type(item, "in", port_name, declared_type)
	return "any"

func update_caption(caption: Control, name: String, value_type: String) -> void:
	var kind := caption.get_child(1) as Label
	kind.text = TYPE_NAMES.get(value_type, "ANY")
	kind.add_theme_color_override("font_color", COLORS.get(value_type, COLORS.any))
	caption.tooltip_text = "%s · %s" % [tr(name.capitalize()), tr(kind.text)]

func port_caption(name: String, value_type: String, output: bool) -> Control:
	var stack := VBoxContainer.new()
	stack.add_theme_constant_override("separation", 0)
	stack.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var title := Label.new()
	title.text = "Run" if name == "flow" else name.capitalize()
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT if output else HORIZONTAL_ALIGNMENT_LEFT
	title.add_theme_font_size_override("font_size", 14)
	title.add_theme_color_override("font_color", Color("e2edf2"))
	stack.add_child(title)
	var kind := Label.new()
	kind.horizontal_alignment = title.horizontal_alignment
	kind.add_theme_font_size_override("font_size", 12)
	stack.add_child(kind)
	update_caption(stack, name, value_type)
	return stack

func refresh_port_visuals() -> void:
	for item in data.nodes:
		if not TYPES.has(str(item.kind)): continue
		var node := canvas.get_node_or_null(str(item.id)) as GraphNode
		if node == null: continue
		var captions: Array = node.get_meta("port_captions", [])
		var inputs: Array = TYPES[item.kind][0]
		var outputs: Array = TYPES[item.kind][1]
		for i in captions.size():
			var input_type: String = shown_port_type(item, "in", str(inputs[i][0]), str(inputs[i][1])) if i < inputs.size() else "any"
			var output_type: String = shown_port_type(item, "out", str(outputs[i][0]), str(outputs[i][1])) if i < outputs.size() else "any"
			if i < inputs.size(): update_caption(captions[i][0], str(inputs[i][0]), input_type)
			if i < outputs.size(): update_caption(captions[i][1], str(outputs[i][0]), output_type)
			node.set_slot(i,i < inputs.size(), PORT_IDS[inputs[i][1]] if i < inputs.size() else 0, COLORS[input_type] if i < inputs.size() else Color.WHITE,
				i < outputs.size(), PORT_IDS[outputs[i][1]] if i < outputs.size() else 0, COLORS[output_type] if i < outputs.size() else Color.WHITE)

func add_node(kind: String, at_position: Vector2 = Vector2.INF) -> void:
	capture_layout()
	if data.nodes.size() >= 64:
		message.text = "Maximum: 64 nodes."
		return
	if kind.begins_with("on_") and data.nodes.any(func(item): return item.kind == kind):
		AppLanguage.format_text(message, "Only one %s event is allowed.", [kind])
		return
	var index := 1
	while data.nodes.any(func(item): return item.id == "n%d" % index): index += 1
	var id := "n%s_%s" % [Time.get_ticks_usec(), randi()] if collaborative_ids else "n%d" % index
	var value: Variant = null
	match kind:
		"number", "public_number": value = 0.0
		"bool": value = false
		"string": value = ""
		"vec3": value = [0.0,0.0,0.0]
		"color": value = [1.0,1.0,1.0,1.0]
		"get_state", "set_state": value = "value"
	var column := (index - 1) % 3
	var row := (index - 1) / 3
	var position := Vector2(float(40 + column * 230),float(80 + row * 180)) if at_position == Vector2.INF else at_position
	data.nodes.append({"id":id,"kind":kind,"x":position.x,"y":position.y,"value":value})
	redraw()
	touch()

func redraw() -> void:
	loading = true
	var scroll := canvas.scroll_offset
	var zoom := canvas.zoom
	component_name.text = str(data.get("name","Graph1"))
	canvas.clear_connections()
	display_labels.clear()
	for child in canvas.get_children():
		if child is GraphNode:
			canvas.remove_child(child)
			child.queue_free()
	for item in data.nodes:
		if not TYPES.has(str(item.kind)): continue
		var node := GraphNode.new()
		node.name = str(item.id)
		node.title = node_label(str(item.kind)) + "  ·  " + str(item.id)
		node.set_meta("title_source", node.title.trim_suffix("  ·  " + str(item.id)))
		node.tooltip_text = MATH_HINTS.get(str(item.kind), "")
		node.custom_minimum_size.x = 190
		node.position_offset = Vector2(float(item.x),float(item.y))
		node.resizable = false
		canvas.add_child(node)
		var inputs: Array = TYPES[item.kind][0]
		var outputs: Array = TYPES[item.kind][1]
		var captions: Array = []
		for i in maxi(inputs.size(),outputs.size()):
			var row := HBoxContainer.new()
			row.custom_minimum_size = Vector2(170,42)
			var input_type: String = shown_port_type(item, "in", str(inputs[i][0]), str(inputs[i][1])) if i < inputs.size() else "any"
			var output_type: String = shown_port_type(item, "out", str(outputs[i][0]), str(outputs[i][1])) if i < outputs.size() else "any"
			var left := port_caption(str(inputs[i][0]), input_type, false) if i < inputs.size() else Control.new()
			var right := port_caption(str(outputs[i][0]), output_type, true) if i < outputs.size() else Control.new()
			row.add_child(left)
			row.add_child(right)
			captions.append([left,right])
			node.add_child(row)
			node.set_slot(i,i < inputs.size(), PORT_IDS[inputs[i][1]] if i < inputs.size() else 0, COLORS[input_type] if i < inputs.size() else Color.WHITE,
				i < outputs.size(), PORT_IDS[outputs[i][1]] if i < outputs.size() else 0, COLORS[output_type] if i < outputs.size() else Color.WHITE)
		node.set_meta("port_captions", captions)
		if item.kind in ["number","public_number","string","get_state","set_state","vec3","color"]:
			var input := LineEdit.new()
			input.custom_minimum_size.x = 170
			input.text = JSON.stringify(item.value) if item.kind in ["vec3","color"] else str(item.value)
			input.placeholder_text = "State key" if item.kind in ["get_state","set_state"] else "Value"
			input.text_changed.connect(func(value):
				if item.kind in ["number","public_number"]: item.value = float(value) if value.is_valid_float() else value
				elif item.kind in ["vec3","color"]: item.value = JSON.parse_string(value)
				else: item.value = value
				touch())
			node.add_child(input)
		elif item.kind == "bool":
			var toggle := CheckBox.new()
			toggle.text = "True"
			toggle.button_pressed = bool(item.value)
			toggle.toggled.connect(func(value): item.value = value; touch())
			node.add_child(toggle)
		elif item.kind == "display":
			var probe := Label.new()
			probe.text = "Connect a value"
			probe.custom_minimum_size.x = 170
			probe.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
			node.add_child(probe)
			display_labels[str(item.id)] = probe
	for link in data.links:
		var a := find_item(str(link.from))
		var b := find_item(str(link.to))
		if a.is_empty() or b.is_empty(): continue
		var out_index := port_index(a,"out",str(link.out))
		var in_index := port_index(b,"in",str(link.input))
		if out_index >= 0 and in_index >= 0: canvas.connect_node(str(link.from),out_index,str(link.to),in_index)
	canvas.zoom = zoom
	canvas.scroll_offset = scroll
	loading = false
	refresh_language()
	next_preview = 0

func find_item(id: String) -> Dictionary:
	for item in data.nodes:
		if item.id == id: return item
	return {}

func port_index(item: Dictionary, side: String, name: String) -> int:
	var ports: Array = TYPES[item.kind][0 if side == "in" else 1]
	for i in ports.size():
		if ports[i][0] == name: return i
	return -1

func connect_ports(from: StringName, out_port: int, to: StringName, in_port: int) -> void:
	var a := find_item(str(from))
	var b := find_item(str(to))
	if a.is_empty() or b.is_empty(): return
	var out_def: Array = TYPES[a.kind][1][out_port]
	var in_def: Array = TYPES[b.kind][0][in_port]
	var output_type := shown_port_type(a, "out", str(out_def[0]), str(out_def[1]))
	var input_type := connection_input_type(b, str(in_def[0]), str(in_def[1]))
	if output_type != input_type and output_type != "any" and input_type != "any":
		AppLanguage.format_text(message, "%s cannot connect to %s. Choose a matching port.", [TYPE_NAMES.get(output_type, "ANY"), TYPE_NAMES.get(input_type, "ANY")])
		return
	var removed: Array = data.links.filter(func(link): return link.to == str(to) and link.input == in_def[0])
	for link in removed:
		var old := find_item(str(link.from))
		if not old.is_empty(): canvas.disconnect_node(str(link.from),port_index(old,"out",str(link.out)),str(to),in_port)
		data.links.erase(link)
	var link := {"from":str(from),"out":out_def[0],"to":str(to),"input":in_def[0]}
	data.links.append(link)
	canvas.connect_node(from,out_port,to,in_port)
	refresh_port_visuals()
	message.text = PORT_HELP
	touch()

func disconnect_ports(from: StringName, out_port: int, to: StringName, in_port: int) -> void:
	var a := find_item(str(from))
	var b := find_item(str(to))
	if a.is_empty() or b.is_empty(): return
	var out_name = TYPES[a.kind][1][out_port][0]
	var in_name = TYPES[b.kind][0][in_port][0]
	data.links = data.links.filter(func(link): return not (link.from == str(from) and link.out == out_name and link.to == str(to) and link.input == in_name))
	canvas.disconnect_node(from,out_port,to,in_port)
	refresh_port_visuals()
	touch()

func delete_nodes(names: Array[StringName]) -> void:
	capture_layout()
	data.nodes = data.nodes.filter(func(item): return not names.has(StringName(item.id)))
	data.links = data.links.filter(func(link): return not names.has(StringName(link.from)) and not names.has(StringName(link.to)))
	redraw()
	touch()

func preview_input(id: String, port: String, stack: Dictionary, component: Dictionary) -> Dictionary:
	for link in data.links:
		if link.to == id and link.input == port:
			return preview_value(str(link.from), stack, component, str(link.out))
	return {}

func finite_number(value: Variant) -> bool:
	return (value is float or value is int) and is_finite(float(value))

func finite_vector(value: Variant, count: int = 3) -> bool:
	return value is Array and value.size() == count and value.all(finite_number)

func preview_atom_valid(value: Variant) -> bool:
	if value == null or value is bool: return true
	if finite_number(value): return true
	if value is String: return value.to_utf8_buffer().size() <= 512
	return value is Array and value.size() >= 2 and value.size() <= 4 and value.all(finite_number)

func preview_atoms_equal(a: Variant, b: Variant) -> bool:
	if finite_number(a) and finite_number(b): return float(a) == float(b)
	if a is Array and b is Array:
		if a.size() != b.size(): return false
		for i in a.size():
			if float(a[i]) != float(b[i]): return false
		return true
	return typeof(a) == typeof(b) and a == b

func preview_inputs(item: Dictionary, stack: Dictionary, component: Dictionary) -> Dictionary:
	var values := {}
	for port in TYPES[item.kind][0]:
		var incoming := preview_input(str(item.id), str(port[0]), stack, component)
		if not incoming.has("value"): return {}
		var value: Variant = incoming.value
		match str(port[1]):
			"number":
				if not finite_number(value): return {}
				value = float(value)
			"vec3", "color":
				if not finite_vector(value, 3 if port[1] == "vec3" else 4): return {}
				if port[1] == "color" and not value.all(func(n): return n >= 0.0 and n <= 1.0): return {}
				value = value.map(func(n): return float(n))
			"bool":
				if not value is bool: return {}
			"string":
				if not value is String or value.to_utf8_buffer().size() > 512: return {}
			"any":
				if not preview_atom_valid(value): return {}
			_: return {}
		values[port[0]] = value
	return values

# Arrays retain GDScript's double precision; Vector3 may use single precision.
# This is an editor preview. Rust validates and evaluates the saved graph.
func preview_vector_length(value: Array) -> float:
	if not finite_vector(value): return INF
	var scale: float = maxf(absf(value[0]), maxf(absf(value[1]), absf(value[2])))
	if scale == 0.0: return 0.0
	return scale * sqrt(pow(value[0] / scale, 2.0) + pow(value[1] / scale, 2.0) + pow(value[2] / scale, 2.0))

func preview_lerp(a: float, b: float, amount: float) -> float:
	var t := clampf(amount, 0.0, 1.0)
	return preview_blend(a, b, t)

func preview_blend(a: float, b: float, t: float) -> float:
	if t == 0.0: return a
	if t == 1.0: return b
	return (1.0 - t) * a + t * b

func preview_fraction(value: float, minimum: float, maximum: float) -> float:
	if value == minimum: return 0.0
	if value == maximum: return 1.0
	var numerator := value - minimum
	var denominator := maximum - minimum
	if is_finite(numerator) and is_finite(denominator): return numerator / denominator if denominator != 0.0 else INF
	var scale := maxf(absf(value), maxf(absf(minimum), absf(maximum)))
	denominator = maximum / scale - minimum / scale
	return (value / scale - minimum / scale) / denominator if denominator != 0.0 else INF

func preview_normalized(value: Array) -> Array:
	var scale: float = maxf(absf(value[0]), maxf(absf(value[1]), absf(value[2])))
	if scale == 0.0: return [0.0, 0.0, 0.0]
	var scaled: Array = value.map(func(n): return n / scale)
	var magnitude := preview_vector_length(scaled)
	return scaled.map(func(n): return n / magnitude)

func preview_whitespace(code: int) -> bool:
	return (code >= 9 and code <= 13) or code in [32, 0x85, 0xa0, 0x1680, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000] or (code >= 0x2000 and code <= 0x200a)

func preview_trim(text: String) -> String:
	var first := 0
	var last := text.length()
	while first < last and preview_whitespace(text.unicode_at(first)): first += 1
	while last > first and preview_whitespace(text.unicode_at(last - 1)): last -= 1
	return text.substr(first, last - first)

func preview_math(item: Dictionary, output_port: String, stack: Dictionary, component: Dictionary) -> Dictionary:
	var values := preview_inputs(item, stack, component)
	if values.is_empty(): return {}
	var result: Variant
	match str(item.kind):
		"abs": result = absf(values.value)
		"floor": result = floorf(values.value)
		"sin": result = sin(values.value)
		"cos": result = cos(values.value)
		"negate": result = -values.value
		"sqrt":
			if values.value < 0.0: return {}
			result = sqrt(values.value)
		"power":
			if (values.a < 0.0 and floorf(values.b) != values.b) or (values.a == 0.0 and values.b < 0.0): return {}
			result = pow(values.a, values.b)
		"round": result = roundf(values.value)
		"sign": result = signf(values.value)
		"tan": result = tan(values.value)
		"asin", "acos":
			if absf(values.value) > 1.0: return {}
			result = asin(values.value) if item.kind == "asin" else acos(values.value)
		"atan2": result = atan2(values.y, values.x)
		"log":
			if values.value <= 0.0: return {}
			result = log(values.value)
		"exp": result = exp(values.value)
		"clamp":
			if values.min > values.max: return {}
			result = clampf(values.value, values.min, values.max)
		"lerp": result = preview_lerp(values.a, values.b, values.t)
		"remap":
			if values.in_min == values.in_max: return {}
			if values.out_min == values.out_max: result = values.out_min
			else:
				var t := preview_fraction(values.value, values.in_min, values.in_max)
				if not is_finite(t): return {}
				if t == 0.0: result = values.out_min
				elif t == 1.0: result = values.out_max
				else:
					var difference: float = values.out_max - values.out_min
					var mapped: float = values.out_min + t * difference
					result = mapped if is_finite(difference) and is_finite(mapped) else preview_blend(values.out_min, values.out_max, t)
		"smoothstep":
			if values.edge0 >= values.edge1: return {}
			var t: float = 0.0 if values.value <= values.edge0 else (1.0 if values.value >= values.edge1 else preview_fraction(values.value, values.edge0, values.edge1))
			if not is_finite(t): return {}
			t = clampf(t, 0.0, 1.0)
			result = t * t * (3.0 - 2.0 * t)
		"ping_pong":
			if values.length <= 0.0: return {}
			var twice: float = values.length * 2.0
			var phase: float = fmod(absf(values.value), twice) if is_finite(twice) else absf(values.value)
			result = clampf(phase if phase <= values.length else values.length - (phase - values.length), 0.0, values.length)
		"less_equal": result = values.a <= values.b
		"greater_equal": result = values.a >= values.b
		"equal", "not_equal":
			result = preview_atoms_equal(values.a, values.b)
			if item.kind == "not_equal": result = not result
		"xor": result = values.a != values.b
		"parse_number":
			var text := preview_trim(values.text)
			if not text.is_valid_float(): return {}
			result = text.to_float()
		"number_to_bool": result = values.value != 0.0
		"bool_to_number": result = 1.0 if values.value else 0.0
		"string_length": result = values.text.length()
		"string_contains": result = values.text.contains(values.part)
		"string_starts_with": result = values.text.begins_with(values.part)
		"string_ends_with": result = values.text.ends_with(values.part)
		"string_slice":
			if values.start < 0.0 or values.count < 0.0 or floorf(values.start) != values.start or floorf(values.count) != values.count: return {}
			var size: int = values.text.length()
			result = values.text.substr(int(minf(values.start, size)), int(minf(values.count, size)))
		"vec3_make": result = [values.x, values.y, values.z]
		"vec3_split":
			var index := ["x", "y", "z"].find(output_port)
			if index < 0: return {}
			result = values.value[index]
		"vec3_scale": result = values.value.map(func(n): return n * values.factor)
		"vec3_length": result = preview_vector_length(values.value)
		"vec3_normalize": result = preview_normalized(values.value)
		"vec3_angle":
			var a := preview_normalized(values.a)
			var b := preview_normalized(values.b)
			if a == [0.0,0.0,0.0] or b == [0.0,0.0,0.0]: return {}
			var cross := [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
			result = atan2(preview_vector_length(cross), a[0]*b[0]+a[1]*b[1]+a[2]*b[2])
		"vec3_reflect":
			var normal := preview_normalized(values.normal)
			if normal == [0.0,0.0,0.0]: return {}
			var scale: float = maxf(absf(values.value[0]), maxf(absf(values.value[1]), absf(values.value[2])))
			if scale == 0.0: result = [0.0,0.0,0.0]
			else:
				var value: Array = values.value.map(func(n): return n / scale)
				var dot: float = value[0]*normal[0]+value[1]*normal[1]+value[2]*normal[2]
				result = [(value[0]-2.0*dot*normal[0])*scale,(value[1]-2.0*dot*normal[1])*scale,(value[2]-2.0*dot*normal[2])*scale]
		"vec3_move_towards":
			if values.max_delta < 0.0: return {}
			var a: Array = values.a
			var b: Array = values.b
			if a == b: result = b
			elif values.max_delta == 0.0: result = a
			else:
				var difference := [b[0]-a[0],b[1]-a[1],b[2]-a[2]]
				var scale: float
				var scaled: Array
				if finite_vector(difference):
					scale = maxf(absf(difference[0]), maxf(absf(difference[1]), absf(difference[2])))
					scaled = difference.map(func(n): return n / scale)
				else:
					scale = maxf(maxf(absf(a[0]),maxf(absf(a[1]),absf(a[2]))), maxf(absf(b[0]),maxf(absf(b[1]),absf(b[2]))))
					scaled = [b[0]/scale-a[0]/scale,b[1]/scale-a[1]/scale,b[2]/scale-a[2]/scale]
				var t: float = minf((values.max_delta / scale) / preview_vector_length(scaled), 1.0)
				result = [preview_blend(a[0],b[0],t),preview_blend(a[1],b[1],t),preview_blend(a[2],b[2],t)]
		"color_make":
			result = [values.r, values.g, values.b, values.a]
			if not result.all(func(n): return n >= 0.0 and n <= 1.0): return {}
		"color_split":
			var index := ["r", "g", "b", "a"].find(output_port)
			if index < 0: return {}
			result = values.value[index]
		"color_lerp":
			result = []
			for i in 4: result.append(preview_lerp(values.a[i], values.b[i], values.t))
		"vec3_add", "vec3_subtract", "vec3_dot", "vec3_cross", "vec3_distance", "vec3_lerp":
			var a: Array = values.a
			var b: Array = values.b
			match str(item.kind):
				"vec3_add": result = [a[0]+b[0], a[1]+b[1], a[2]+b[2]]
				"vec3_subtract": result = [a[0]-b[0], a[1]-b[1], a[2]-b[2]]
				"vec3_dot": result = a[0]*b[0]+a[1]*b[1]+a[2]*b[2]
				"vec3_cross": result = [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
				"vec3_distance": result = preview_vector_length([a[0]-b[0], a[1]-b[1], a[2]-b[2]])
				"vec3_lerp": result = [preview_lerp(a[0],b[0],values.t), preview_lerp(a[1],b[1],values.t), preview_lerp(a[2],b[2],values.t)]
	return {"value":result} if result is bool or result is String or finite_number(result) or finite_vector(result) or finite_vector(result,4) else {}

func preview_value(id: String, stack: Dictionary, component: Dictionary, output_port: String = "value") -> Dictionary:
	if stack.has(id) or stack.size() >= 64: return {}
	var item := find_item(id)
	if item.is_empty(): return {}
	stack[id] = true
	var result: Dictionary = {}
	if MATH_HINTS.has(str(item.kind)) and item.kind not in ["branch","for_range","while_loop","break_loop","continue_loop"]:
		result = preview_math(item, output_port, stack, component)
		stack.erase(id)
		return result
	match str(item.kind):
		"number":
			if finite_number(item.value): result = {"value":item.value}
		"bool":
			if item.value is bool: result = {"value":item.value}
		"string":
			if item.value is String: result = {"value":item.value}
		"vec3", "color":
			if finite_vector(item.value, 3 if item.kind == "vec3" else 4): result = {"value":item.value}
		"public_number":
			result = {"value":component.get("parameters",{}).get("p_" + id,item.value)}
		"get_state":
			result = {"value":component.get("state",{}).get(str(item.value),0)}
		"read_field":
			var reference = component.get("parameters",{}).get("ref_" + id)
			if reference is Dictionary and reference.get("kind") == "field":
				var address: Dictionary = reference.get("target",{})
				for field in app.objects.get(str(address.get("instance_id","")),{}).get("fields",[]):
					if field.id == reference.get("field_id") and int(field.component_id) == int(address.get("local_id",-1)):
						result = {"value":field.value}
						break
		"time", "world_time":
			if app != null and app.clock_ready:
				var now: float = app.media_time()
				var clock: Dictionary = component.get("clock",{})
				result = {"value":now if item.kind == "world_time" else float(clock.get("elapsed",0.0)) + (maxf(0.0,now-float(clock.get("anchor",0.0))) if component.get("enabled",false) else 0.0)}
		"display":
			result = preview_input(id,"value",stack,component)
		"not", "to_string", "ceil":
			var a := preview_input(id,"a" if item.kind == "not" else "value",stack,component)
			if a.has("value"):
				if item.kind == "not" and a.value is bool: result = {"value":not a.value}
				elif item.kind == "to_string" and not (a.value is Array or a.value is Dictionary): result = {"value":str(a.value)}
				elif item.kind == "ceil" and (a.value is float or a.value is int): result = {"value":ceilf(float(a.value))}
		"select":
			var test := preview_input(id,"test",stack,component)
			if test.has("value") and test.value is bool:
				result = preview_input(id,"yes" if bool(test.value) else "no",stack,component)
		"rotation_y":
			var degrees := preview_input(id,"degrees",stack,component)
			var base := preview_input(id,"base",stack,component)
			if degrees.has("value") and base.has("value") and (degrees.value is float or degrees.value is int) and base.value is Array and base.value.size() == 4:
				var q: Quaternion = (Quaternion(Vector3.UP,deg_to_rad(float(degrees.value))) * Quaternion(float(base.value[0]),float(base.value[1]),float(base.value[2]),float(base.value[3]))).normalized()
				result = {"value":[q.x,q.y,q.z,q.w]}
		"for_range", "while_loop", "break_loop", "continue_loop":
			pass
		_:
			var a := preview_input(id,"a",stack,component)
			var b := preview_input(id,"b",stack,component)
			if a.has("value") and b.has("value"):
				var x: Variant = a.value
				var y: Variant = b.value
				match str(item.kind):
					"add": if (x is float or x is int) and (y is float or y is int): result = {"value":float(x)+float(y)}
					"subtract": if (x is float or x is int) and (y is float or y is int): result = {"value":float(x)-float(y)}
					"multiply": if (x is float or x is int) and (y is float or y is int): result = {"value":float(x)*float(y)}
					"divide": if (x is float or x is int) and (y is float or y is int) and float(y) != 0.0: result = {"value":float(x)/float(y)}
					"modulo": if (x is float or x is int) and (y is float or y is int) and float(y) != 0.0: result = {"value":fmod(float(x),float(y))}
					"min": if (x is float or x is int) and (y is float or y is int): result = {"value":minf(float(x),float(y))}
					"max": if (x is float or x is int) and (y is float or y is int): result = {"value":maxf(float(x),float(y))}
					"less": if (x is float or x is int) and (y is float or y is int): result = {"value":float(x)<float(y)}
					"greater": if (x is float or x is int) and (y is float or y is int): result = {"value":float(x)>float(y)}
					"and": if x is bool and y is bool: result = {"value":x and y}
					"or": if x is bool and y is bool: result = {"value":x or y}
					"join": result = {"value":str(x)+str(y)}
	stack.erase(id)
	if result.has("value") and (result.value is float or result.value is int) and not finite_number(result.value): return {}
	return result

func tick_preview() -> void:
	if not visible or display_labels.is_empty() or app == null or Time.get_ticks_msec() < next_preview: return
	next_preview = Time.get_ticks_msec() + 100
	var component: Dictionary = {}
	for candidate in app.objects.get(target_id,{}).get("scripts",[]):
		if candidate.id == script_id:
			component = candidate
			break
	if component.is_empty(): return
	for id in display_labels:
		var value := preview_input(id,"value",{},component)
		var label: Label = display_labels[id]
		var shown := "—" if not value.has("value") else (JSON.stringify(value.value) if value.value is Array or value.value is Dictionary else str(value.value))
		label.auto_translate_mode = Node.AUTO_TRANSLATE_MODE_DISABLED
		label.text = shown.substr(0,80)
		label.tooltip_text = shown
