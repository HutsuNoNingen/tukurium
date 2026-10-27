extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func until(test: Callable, message: String) -> bool:
	for _i in 600:
		await create_timer(0.025).timeout
		if test.call(): return true
	push_error(message)
	return false

func close_enough(actual: Variant, expected: Variant) -> bool:
	if expected is bool or expected is String: return typeof(actual) == typeof(expected) and actual == expected
	if expected is Array:
		if not actual is Array or actual.size() != expected.size(): return false
		for i in expected.size():
			if not close_enough(actual[i], expected[i]): return false
		return true
	if (expected is int or expected is float) and expected != 0 and absf(float(expected)) < 1e-100:
		return (actual is int or actual is float) and actual == expected
	return (actual is int or actual is float) and is_finite(float(actual)) and absf(float(actual)-float(expected)) <= 1e-10 * maxf(1.0, absf(float(expected)))

func preview_case(graph, kind: String, values: Dictionary, expected: Variant, output: String = "value") -> bool:
	var nodes: Array = [{"id":"math","kind":kind,"x":0,"y":0,"value":null}]
	var links: Array = []
	var state := {}
	for port in graph.TYPES[kind][0]:
		var id := "input_" + str(port[0])
		var literal_kind: String = port[1]
		var value: Variant = values[port[0]]
		if literal_kind == "any":
			literal_kind = "get_state"
			state[id] = value
			value = id
		nodes.append({"id":id,"kind":literal_kind,"x":0,"y":0,"value":value})
		links.append({"from":id,"out":"value","to":"math","input":port[0]})
	graph.data = {"name":"Preview","nodes":nodes,"links":links}
	var result: Dictionary = graph.preview_value("math", {}, {"state":state}, output)
	if expected == null:
		if result.is_empty(): return true
	elif result.has("value") and close_enough(result.value, expected): return true
	push_error("Math preview %s/%s: %s, expected %s" % [kind, output, result, expected])
	return false

func check_previews(graph) -> bool:
	var cases := [
		["abs", {"value":-3}, 3], ["floor", {"value":-1.2}, -2],
		["sin", {"value":PI/2.0}, 1], ["cos", {"value":PI}, -1],
		["clamp", {"value":-3,"min":0,"max":2}, 0],
		["clamp", {"value":2,"min":2,"max":2}, 2],
		["lerp", {"a":2,"b":6,"t":2}, 6],
		["lerp", {"a":-1e308,"b":1e308,"t":0.5}, 0],
		["vec3_make", {"x":1,"y":2,"z":3}, [1,2,3]],
		["vec3_split", {"value":[1,2,3]}, 3, "z"],
		["vec3_add", {"a":[1,2,3],"b":[4,5,6]}, [5,7,9]],
		["vec3_subtract", {"a":[1,2,3],"b":[4,5,6]}, [-3,-3,-3]],
		["vec3_scale", {"value":[16777217,2,3],"factor":1}, [16777217,2,3]],
		["vec3_dot", {"a":[1,2,3],"b":[4,5,6]}, 32],
		["vec3_cross", {"a":[1,2,3],"b":[4,5,6]}, [-3,6,-3]],
		["vec3_length", {"value":[3,4,0]}, 5],
		["vec3_length", {"value":[1e308,0,0]}, 1e308],
		["vec3_normalize", {"value":[0,0,0]}, [0,0,0]],
		["vec3_normalize", {"value":[1e-308,0,0]}, [1,0,0]],
		["vec3_normalize", {"value":[1e308,0,0]}, [1,0,0]],
		["vec3_distance", {"a":[1,2,3],"b":[4,6,3]}, 5],
		["vec3_lerp", {"a":[1,2,3],"b":[4,5,6],"t":-1}, [1,2,3]],
		["vec3_lerp", {"a":[1,2,3],"b":[4,5,6],"t":2}, [4,5,6]],
		["clamp", {"value":1,"min":2,"max":0}, null],
		["sin", {"value":INF}, null], ["abs", {"value":"3"}, null],
		["vec3_length", {"value":[1,2]}, null],
		["vec3_length", {"value":[1,"2",3]}, null],
		["vec3_scale", {"value":[1e308,0,0],"factor":1e308}, null],
		["vec3_dot", {"a":[1e308,0,0],"b":[1e308,0,0]}, null],
		["vec3_distance", {"a":[1e308,0,0],"b":[-1e308,0,0]}, null],
		["negate", {"value":-3}, 3],
		["sqrt", {"value":9}, 3],
		["power", {"a":2,"b":3}, 8],
		["round", {"value":-1.5}, -2],
		["sign", {"value":0}, 0],
		["tan", {"value":PI/4}, 1],
		["asin", {"value":1}, PI/2],
		["acos", {"value":-1}, PI],
		["atan2", {"y":1,"x":0}, PI/2],
		["log", {"value":1}, 0],
		["exp", {"value":0}, 1],
		["remap", {"value":5,"in_min":0,"in_max":10,"out_min":10,"out_max":20}, 15],
		["remap", {"value":0,"in_min":-1e308,"in_max":1e308,"out_min":0,"out_max":1}, 0.5],
		["remap", {"value":1,"in_min":0,"in_max":1,"out_min":-1e308,"out_max":1e-308}, 1e-308],
		["smoothstep", {"edge0":0,"edge1":2,"value":1}, 0.5],
		["smoothstep", {"edge0":-1e308,"edge1":1e308,"value":0}, 0.5],
		["ping_pong", {"value":-4,"length":3}, 2],
		["less_equal", {"a":3,"b":3}, true],
		["greater_equal", {"a":2,"b":3}, false],
		["not_equal", {"a":[1,2,3],"b":[1.0,2.0,3.0]}, false],
		["equal", {"a":null,"b":null}, true],
		["equal", {"a":1e308,"b":1e308}, true],
		["equal", {"a":[1e308,2],"b":[1e308,2.0]}, true],
		["not_equal", {"a":false,"b":0}, true],
		["not_equal", {"a":[1,2],"b":[1,2,3]}, true],
		["xor", {"a":true,"b":false}, true],
		["parse_number", {"text":"\u00a0\u3000-1.25e2\u3000"}, -125],
		["number_to_bool", {"value":-0.0}, false],
		["number_to_bool", {"value":-1}, true],
		["bool_to_number", {"value":true}, 1],
		["string_length", {"text":"가日🙂"}, 3],
		["string_contains", {"text":"가日🙂","part":"日🙂"}, true],
		["string_starts_with", {"text":"가日🙂","part":"가"}, true],
		["string_ends_with", {"text":"가日🙂","part":"🙂"}, true],
		["string_slice", {"text":"가日🙂","start":1,"count":2}, "日🙂"],
		["string_slice", {"text":"가日🙂","start":99,"count":2}, ""],
		["vec3_angle", {"a":[1,0,0],"b":[0,1,0]}, PI/2],
		["vec3_reflect", {"value":[1,-2,3],"normal":[0,2,0]}, [1,2,3]],
		["vec3_move_towards", {"a":[0,0,0],"b":[3,4,0],"max_delta":2}, [1.2,1.6,0]],
		["vec3_move_towards", {"a":[-1e308,0,0],"b":[1e308,0,0],"max_delta":1e308}, [0,0,0]],
		["color_make", {"r":0.1,"g":0.2,"b":0.3,"a":0.4}, [0.1,0.2,0.3,0.4]],
		["color_lerp", {"a":[1,0,0,1],"b":[0,0,1,0],"t":0.5}, [0.5,0,0.5,0.5]],
		["sqrt", {"value":-1}, null],
		["power", {"a":-2,"b":0.5}, null],
		["asin", {"value":2}, null],
		["acos", {"value":-2}, null],
		["log", {"value":0}, null],
		["exp", {"value":1000}, null],
		["remap", {"value":1,"in_min":2,"in_max":2,"out_min":0,"out_max":1}, null],
		["smoothstep", {"edge0":2,"edge1":1,"value":1}, null],
		["ping_pong", {"value":1,"length":0}, null],
		["xor", {"a":1,"b":false}, null],
		["parse_number", {"text":"0x20"}, null],
		["parse_number", {"text":"NaN"}, null],
		["parse_number", {"text":"1_000"}, null],
		["parse_number", {"text":"not a number"}, null],
		["string_length", {"text":"🙂".repeat(129)}, null],
		["string_slice", {"text":"가日🙂","start":0.5,"count":1}, null],
		["string_slice", {"text":"가日🙂","start":0,"count":-1}, null],
		["vec3_angle", {"a":[0,0,0],"b":[0,1,0]}, null],
		["vec3_reflect", {"value":[1,2,3],"normal":[0,0,0]}, null],
		["vec3_move_towards", {"a":[0,0,0],"b":[1,2,3],"max_delta":-1}, null],
		["color_make", {"r":1.1,"g":0,"b":0,"a":1}, null],
		["color_lerp", {"a":[1,0,0],"b":[0,0,1,1],"t":0.5}, null],
		["equal", {"a":{"bad":1},"b":{"bad":1}}, null],
		["equal", {"a":[1,2,3,4,5],"b":[1,2,3,4,5]}, null]
	]
	for item in cases:
		if not preview_case(graph, item[0], item[1], item[2], item[3] if item.size() > 3 else "value"): return false
	for channel in 4:
		if not preview_case(graph, "color_split", {"value":[0.1,0.2,0.3,0.4]}, (channel+1)*0.1, ["r","g","b","a"][channel]): return false
	if graph.TYPES.size() != 97: push_error("Expected 97 registered node kinds"); return false
	graph.data = {"name":"RuntimeOnly","nodes":[{"id":"range","kind":"for_range","x":0,"y":0}],"links":[]}
	if not graph.preview_value("range", {}, {}, "index").is_empty(): push_error("For index must be unavailable outside runtime"); return false
	graph.data = {"name":"Limits","nodes":[],"links":[]}
	for i in 64: graph.data.nodes.append({"id":"limit%d" % i,"kind":"number","x":0,"y":0,"value":0})
	graph.add_node("number")
	if graph.data.nodes.size() != 64 or not graph.message.text.contains("64"): push_error("Graph node limit did not reject the 65th node"); return false
	return true

func click_control(viewport: Viewport, control: BaseButton) -> bool:
	await process_frame
	var point := control.get_global_rect().get_center()
	var input_viewport := viewport
	# Embedded Windows receive OS pointer events through their parent Viewport.
	# Route the synthetic pointer through that same path in --single-window mode.
	if viewport is Window and viewport.is_embedded():
		input_viewport = viewport.get_parent().get_viewport()
		point += Vector2(viewport.position)
	var clicked := {"count":0}
	control.pressed.connect(func(): clicked.count += 1, CONNECT_ONE_SHOT)
	var motion := InputEventMouseMotion.new()
	motion.position = point
	motion.global_position = point
	input_viewport.push_input(motion, true)
	await process_frame
	for pressed in [true, false]:
		var event := InputEventMouseButton.new()
		event.button_index = MOUSE_BUTTON_LEFT
		event.position = point
		event.global_position = point
		event.pressed = pressed
		input_viewport.push_input(event, true)
	await process_frame
	if clicked.count == 1: return true
	push_error("Pointer click did not activate " + control.text)
	return false

func add_from_palette(graph, kind: String) -> String:
	graph.palette.select(graph.TYPES.keys().find(kind))
	var tools: HBoxContainer = graph.get_child(0)
	tools.get_child(tools.get_child_count()-1).pressed.emit()
	return str(graph.data.nodes[-1].id)

func set_node_value(graph, id: String, value: Variant) -> void:
	var input: LineEdit = graph.canvas.get_node(id).find_children("*", "LineEdit", true, false)[0]
	input.text = JSON.stringify(value) if value is Array else str(value)
	input.text_changed.emit(input.text)

func add_literal(graph, kind: String, value: Variant) -> String:
	var id := add_from_palette(graph, kind)
	set_node_value(graph, id, value)
	return id

func add_base_checks(graph) -> Dictionary:
	graph.fresh("MathNodes")
	graph.component_name.text_changed.emit("MathNodes")
	var source := add_literal(graph,"vec3",[1,2,3])
	graph.open_context_menu(Vector2(240,40))
	graph.context_search.text = "split"
	graph.context_search.text_changed.emit("split")
	if not graph.context_buttons.vec3_split.visible or graph.context_buttons.sin.visible:
		push_error("Math node context search did not filter labels"); return {}
	graph.context_buttons.vec3_split.pressed.emit()
	var split: String = graph.data.nodes[-1].id
	var make := add_from_palette(graph,"vec3_make")
	var event := add_from_palette(graph,"on_interact")
	var state := add_literal(graph,"set_state","math_result")
	var display := add_from_palette(graph,"display")
	graph.connect_ports(source,0,split,0)
	graph.connect_ports(split,2,make,0)
	graph.connect_ports(split,0,make,1)
	graph.connect_ports(split,1,make,2)
	graph.connect_ports(make,0,state,1)
	graph.connect_ports(event,0,state,0)
	graph.connect_ports(make,0,display,0)
	return {"source":source,"split":split,"make":make,"event":event,"state":state,"display":display}

func add_flow_checks(graph, event: String, math_state: String) -> bool:
	var sequence := add_from_palette(graph, "sequence")
	graph.connect_ports(event,0,sequence,0)
	graph.connect_ports(sequence,0,math_state,0)
	var color := add_literal(graph,"color",[0.1,0.2,0.3,0.4])
	var split := add_from_palette(graph,"color_split")
	var make := add_from_palette(graph,"color_make")
	var color_state := add_literal(graph,"set_state","color_result")
	graph.connect_ports(color,0,split,0)
	for i in 4: graph.connect_ports(split,(i+3)%4,make,i)
	graph.connect_ports(make,0,color_state,1)
	graph.connect_ports(math_state,0,color_state,0)
	var split_view: GraphNode = graph.canvas.get_node(split)
	if split_view.get_output_port_count() != 4: push_error("Split Color must expose four NUMBER outputs"); return false
	for i in 4:
		if split_view.get_meta("port_captions")[i][1].get_child(1).text != "NUMBER": push_error("Color channel output lost its NUMBER type"); return false
	var zero := add_literal(graph,"number",0)
	var one := add_literal(graph,"number",1)
	var four := add_literal(graph,"number",4)
	var thirteen := add_literal(graph,"number",13)
	var twenty := add_literal(graph,"number",20)
	var initialize := add_literal(graph,"set_state","loop_result")
	var get_sum := add_literal(graph,"get_state","loop_result")
	var range_loop := add_from_palette(graph,"for_range")
	var body := add_from_palette(graph,"sequence")
	var sum := add_from_palette(graph,"add")
	var sum_state := add_literal(graph,"set_state","loop_result")
	var next_index := add_from_palette(graph,"continue_loop")
	# Continue must skip Sequence.second, which would corrupt the result.
	var skipped := add_literal(graph,"set_state","loop_result")
	var sentinel := add_literal(graph,"number",999)
	graph.connect_ports(sequence,1,initialize,0)
	graph.connect_ports(zero,0,initialize,1)
	graph.connect_ports(initialize,0,range_loop,0)
	graph.connect_ports(zero,0,range_loop,1)
	graph.connect_ports(four,0,range_loop,2)
	graph.connect_ports(one,0,range_loop,3)
	graph.connect_ports(range_loop,0,body,0)
	graph.connect_ports(body,0,sum_state,0)
	graph.connect_ports(get_sum,0,sum,0)
	graph.connect_ports(range_loop,2,sum,1)
	graph.connect_ports(sum,0,sum_state,1)
	graph.connect_ports(sum_state,0,next_index,0)
	graph.connect_ports(body,1,skipped,0)
	graph.connect_ports(sentinel,0,skipped,1)
	var while_loop := add_from_palette(graph,"while_loop")
	var less := add_from_palette(graph,"less")
	var branch := add_from_palette(graph,"branch")
	var enough := add_from_palette(graph,"greater_equal")
	var stop := add_from_palette(graph,"break_loop")
	var increment := add_from_palette(graph,"add")
	var increment_state := add_literal(graph,"set_state","loop_result")
	var next_while := add_from_palette(graph,"continue_loop")
	var done := add_literal(graph,"set_state","loop_done")
	var yes := add_from_palette(graph,"bool")
	var yes_input: CheckBox = graph.canvas.get_node(yes).find_children("*","CheckBox",true,false)[0]
	yes_input.button_pressed = true
	graph.connect_ports(range_loop,1,while_loop,0)
	graph.connect_ports(get_sum,0,less,0)
	graph.connect_ports(twenty,0,less,1)
	graph.connect_ports(less,0,while_loop,1)
	graph.connect_ports(while_loop,0,branch,0)
	graph.connect_ports(get_sum,0,enough,0)
	graph.connect_ports(thirteen,0,enough,1)
	graph.connect_ports(enough,0,branch,1)
	graph.connect_ports(branch,0,stop,0)
	graph.connect_ports(branch,1,increment_state,0)
	graph.connect_ports(get_sum,0,increment,0)
	graph.connect_ports(one,0,increment,1)
	graph.connect_ports(increment,0,increment_state,1)
	graph.connect_ports(increment_state,0,next_while,0)
	graph.connect_ports(while_loop,1,done,0)
	graph.connect_ports(yes,0,done,1)
	graph.filter_context_menu("if / else")
	if not graph.context_buttons.branch.visible: push_error("If / Else was not searchable"); return false
	graph.filter_context_menu("flow / loops")
	for kind in ["for_range","while_loop","break_loop","continue_loop"]:
		if not graph.context_buttons[kind].visible: push_error("Loop category search omitted " + kind); return false
	graph.filter_context_menu("")
	return true

func run() -> void:
	if "--preview-only" in OS.get_cmdline_user_args():
		var preview = load("res://script_graph.gd").new()
		root.add_child(preview)
		if not check_previews(preview): quit(1); return
		var nodes := add_base_checks(preview)
		if nodes.is_empty(): quit(1); return
		if not add_flow_checks(preview, nodes.event, nodes.state): quit(1); return
		var bytes: int = preview.source().to_utf8_buffer().size()
		if bytes > 8192: push_error("Preview math/loop graph exceeds source limit: " + str(bytes)); quit(1); return
		print("MATH PREVIEWS PASS: %d node graph, %d bytes" % [preview.data.nodes.size(),bytes])
		preview.queue_free()
		await process_frame
		await process_frame
		quit(0)
		return
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	if not await until(func(): return scene.editable and not scene.current.is_empty(), "Math scene did not connect"): quit(1); return
	var id: String = scene.selected_id
	var editor = scene.script_editor
	editor.open(id)
	if editor.graph_button.disabled: push_error("No free script slot for math graph"); quit(1); return
	var script_count: int = scene.objects[id].get("scripts", []).size()
	# Use an actual pointer click in the existing editor window.
	if not await click_control(editor.window, editor.graph_button): quit(1); return
	if not await until(func(): return editor.pending_id.is_empty() and editor.is_graph() and scene.objects[id].get("scripts", []).size() == script_count + 1, "Math graph was not created through the UI"): quit(1); return
	var graph = editor.graph
	if not check_previews(graph): quit(1); return
	var nodes := add_base_checks(graph)
	if nodes.is_empty(): quit(1); return
	var source: String = nodes.source
	var split: String = nodes.split
	var make: String = nodes.make
	var event: String = nodes.event
	var state: String = nodes.state
	var display: String = nodes.display
	var links: int = graph.data.links.size()
	graph.connect_ports(split,0,split,0)
	if graph.data.links.size() != links or not graph.message.text.contains("NUMBER cannot connect to VECTOR 3"):
		push_error("Vector math accepted a mismatched scalar connection"); quit(1); return
	graph.message.text = graph.PORT_HELP
	var split_view: GraphNode = graph.canvas.get_node(split)
	if split_view.get_output_port_count() != 3 or split_view.get_meta("port_captions")[2][1].get_child(1).text != "NUMBER":
		push_error("Split Vector 3 does not expose three typed scalar outputs"); quit(1); return
	for i in 3:
		var result: Dictionary = graph.preview_value(split, {}, {}, ["x","y","z"][i])
		if not result.has("value") or result.value != i+1: push_error("Split preview lost its output port"); quit(1); return
	graph.next_preview = 0
	graph.tick_preview()
	if not close_enough(JSON.parse_string(graph.display_labels[display].text), [3,1,2]):
		push_error("Display did not preview reordered vector: " + graph.display_labels[display].text); quit(1); return
	if not add_flow_checks(graph, event, state): quit(1); return
	if graph.data.nodes.size() > 64 or graph.data.links.size() > 128 or graph.source().to_utf8_buffer().size() > 8192:
		push_error("Combined math/color/loop graph exceeds graph limits: nodes=%d links=%d bytes=%d" % [graph.data.nodes.size(),graph.data.links.size(),graph.source().to_utf8_buffer().size()]); quit(1); return
	# Keep the graph compact and readable in the rendered test artifact.
	var layout := {source:Vector2(20,40),split:Vector2(250,40),make:Vector2(500,40),display:Vector2(750,40),event:Vector2(250,300),state:Vector2(500,300)}
	for node_id in layout: graph.canvas.get_node(node_id).position_offset = layout[node_id]
	graph.canvas.get_node("n1").position_offset = Vector2(20,300)
	graph.capture_layout()
	graph.touch()
	graph.canvas.zoom = 0.7
	graph.canvas.scroll_offset = Vector2.ZERO
	await process_frame
	await process_frame
	if not await click_control(editor.window, editor.apply_button): quit(1); return
	if not await until(func(): return editor.pending_id.is_empty() and not editor.source_dirty and editor.current_script().schema.name == "MathNodes", "Math graph Apply failed: " + editor.feedback.text): quit(1); return
	if not await click_control(editor.window, editor.click_button): quit(1); return
	if not await until(func():
		var result: Dictionary = editor.current_script().get("state",{})
		return close_enough(result.get("math_result"), [3,1,2]) and close_enough(result.get("color_result"), [0.4,0.1,0.2,0.3]) and result.get("loop_result") == 13 and result.get("loop_done") == true,
		"Math, color and loop graph did not execute through Rust"): quit(1); return
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--screenshot="):
			await process_frame
			await process_frame
			editor.window.get_texture().get_image().save_png(arg.trim_prefix("--screenshot="))
	print("MATH NODES SMOKE PASS: scalar/vector/text/color previews, typed split outputs, UI Apply, math_result=[3,1,2] and bounded for/while/if/break/continue loop_result=13")
	quit(0)
