extends VBoxContainer
## Visual source editor. The graph JSON is stored in ScriptComponent.source.
signal changed
const PREFIX := "@tukurium-graph-v1\n"
const TYPES := {
	"on_update":[[], [["flow","flow"]]], "on_interact":[[], [["flow","flow"]]], "on_drive":[[], [["flow","flow"]]],
	"branch":[[["flow","flow"],["test","bool"]],[["yes","flow"],["no","flow"]]],
	"sequence":[[["flow","flow"]],[["first","flow"],["second","flow"]]],
	"write_field":[[["flow","flow"],["value","any"]],[["next","flow"]]],
	"drive_field":[[["flow","flow"],["value","any"]],[["next","flow"]]],
	"set_state":[[["flow","flow"],["value","any"]],[["next","flow"]]],
	"number":[[],[["value","number"]]], "public_number":[[],[["value","number"]]],
	"bool":[[],[["value","bool"]]], "string":[[],[["value","string"]]],
	"vec3":[[],[["value","vec3"]]], "color":[[],[["value","color"]]],
	"delta":[[],[["value","number"]]], "time":[[],[["value","number"]]], "world_time":[[],[["value","number"]]],
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
	"select":[[["test","bool"],["yes","any"],["no","any"]],[["value","any"]]],
	"to_string":[[["value","any"]],[["value","string"]]],
	"join":[[["a","string"],["b","string"]],[["value","string"]]],
	"rotation_y":[[["degrees","number"],["base","any"]],[["value","any"]]]
}
const COLORS := {"flow":Color("f0b86b"),"number":Color("7bd8cb"),"bool":Color("b5a6ff"),"string":Color("e7aeb6"),"vec3":Color("9cc7fa"),"color":Color("f0d891"),"any":Color("cbd5dc")}
const PORT_IDS := {"flow":0,"number":1,"bool":2,"string":3,"vec3":4,"color":5,"any":6}
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
		var category := "Event" if kind.begins_with("on_") else ("Flow" if kind in ["branch","sequence","write_field","drive_field","set_state"] else ("Value" if kind in ["number","public_number","bool","string","vec3","color","delta","time","world_time","read_field","get_state"] else ("Convert" if kind in ["to_string","join","rotation_y"] else "Math / Logic")))
		palette.add_item(category + " · " + kind.replace("_", " ").capitalize())
	tools.add_child(palette)
	var add := Button.new()
	add.text = "+ Node"
	add.pressed.connect(func(): add_node(TYPES.keys()[palette.selected]))
	tools.add_child(add)
	message = Label.new()
	message.text = "Right-click empty space to add a node. Drag ports to connect; Delete removes selected nodes."
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
		choice.text = kind.replace("_", " ").capitalize()
		choice.alignment = HORIZONTAL_ALIGNMENT_LEFT
		choice.focus_mode = Control.FOCUS_NONE
		choice.pressed.connect(func():
			add_node(kind, context_position)
			context_panel.hide())
		menu_list.add_child(choice)
		context_buttons[kind] = choice
	context_panel.hide()

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
		context_buttons[kind].visible = query.is_empty() or str(kind).containsn(query) or str(context_buttons[kind].text).containsn(query)

func capture_layout() -> void:
	for item in data.nodes:
		var view := canvas.get_node_or_null(str(item.id)) as GraphNode
		if view != null:
			item.x = view.position_offset.x
			item.y = view.position_offset.y

func fresh(name: String) -> void:
	data = {"name":name,"nodes":[{"id":"n1","kind":"on_update","x":40.0,"y":80.0}],"links":[]}
	redraw()

func load_source(source: String) -> void:
	if not source.begins_with(PREFIX): return
	var parsed = JSON.parse_string(source.substr(PREFIX.length()))
	if parsed is Dictionary and parsed.has("nodes") and parsed.has("links"):
		data = parsed.duplicate(true)
		redraw()

func source() -> String:
	capture_layout()
	return PREFIX + JSON.stringify(data)

func touch() -> void:
	if not loading: changed.emit()

func add_node(kind: String, at_position: Vector2 = Vector2.INF) -> void:
	capture_layout()
	if data.nodes.size() >= 64:
		message.text = "Maximum: 64 nodes."
		return
	if kind.begins_with("on_") and data.nodes.any(func(item): return item.kind == kind):
		message.text = "Only one %s event is allowed." % kind
		return
	var index := 1
	while data.nodes.any(func(item): return item.id == "n%d" % index): index += 1
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
	data.nodes.append({"id":"n%d" % index,"kind":kind,"x":position.x,"y":position.y,"value":value})
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
		node.title = str(item.kind).replace("_", " ").capitalize() + "  ·  " + str(item.id)
		node.custom_minimum_size.x = 190
		node.position_offset = Vector2(float(item.x),float(item.y))
		node.resizable = false
		canvas.add_child(node)
		var inputs: Array = TYPES[item.kind][0]
		var outputs: Array = TYPES[item.kind][1]
		for i in maxi(inputs.size(),outputs.size()):
			var row := HBoxContainer.new()
			row.custom_minimum_size = Vector2(170,30)
			var left := Label.new()
			left.text = str(inputs[i][0]) if i < inputs.size() else ""
			left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			row.add_child(left)
			var right := Label.new()
			right.text = str(outputs[i][0]) if i < outputs.size() else ""
			row.add_child(right)
			node.add_child(row)
			node.set_slot(i,i < inputs.size(), PORT_IDS[inputs[i][1]] if i < inputs.size() else 0, COLORS[inputs[i][1]] if i < inputs.size() else Color.WHITE,
				i < outputs.size(), PORT_IDS[outputs[i][1]] if i < outputs.size() else 0, COLORS[outputs[i][1]] if i < outputs.size() else Color.WHITE)
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
	if out_def[1] != in_def[1] and out_def[1] != "any" and in_def[1] != "any": return
	var removed: Array = data.links.filter(func(link): return link.to == str(to) and link.input == in_def[0])
	for link in removed:
		var old := find_item(str(link.from))
		if not old.is_empty(): canvas.disconnect_node(str(link.from),port_index(old,"out",str(link.out)),str(to),in_port)
		data.links.erase(link)
	var link := {"from":str(from),"out":out_def[0],"to":str(to),"input":in_def[0]}
	data.links.append(link)
	canvas.connect_node(from,out_port,to,in_port)
	touch()

func disconnect_ports(from: StringName, out_port: int, to: StringName, in_port: int) -> void:
	var a := find_item(str(from))
	var b := find_item(str(to))
	if a.is_empty() or b.is_empty(): return
	var out_name = TYPES[a.kind][1][out_port][0]
	var in_name = TYPES[b.kind][0][in_port][0]
	data.links = data.links.filter(func(link): return not (link.from == str(from) and link.out == out_name and link.to == str(to) and link.input == in_name))
	canvas.disconnect_node(from,out_port,to,in_port)
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
			return preview_value(str(link.from), stack, component)
	return {}

func preview_value(id: String, stack: Dictionary, component: Dictionary) -> Dictionary:
	if stack.has(id) or stack.size() >= 64: return {}
	var item := find_item(id)
	if item.is_empty(): return {}
	stack[id] = true
	var result: Dictionary = {}
	match str(item.kind):
		"number", "bool", "string", "vec3", "color":
			result = {"value":item.value}
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
		"not", "to_string":
			var a := preview_input(id,"a" if item.kind == "not" else "value",stack,component)
			if a.has("value"):
				if item.kind == "not" and a.value is bool: result = {"value":not a.value}
				elif item.kind == "to_string" and not (a.value is Array or a.value is Dictionary): result = {"value":str(a.value)}
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
					"equal": if not (x is Array or x is Dictionary or y is Array or y is Dictionary): result = {"value":x==y}
					"and": if x is bool and y is bool: result = {"value":x and y}
					"or": if x is bool and y is bool: result = {"value":x or y}
					"join": result = {"value":str(x)+str(y)}
	stack.erase(id)
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
		label.text = shown.substr(0,80)
		label.tooltip_text = shown
