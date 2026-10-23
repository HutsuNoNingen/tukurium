extends VBoxContainer

class SlotTree extends Tree:
	var controller
	func _get_drag_data(at: Vector2) -> Variant:
		var item := get_item_at_position(at)
		if item == null or item.get_metadata(0) == null: return null
		var id: String = item.get_metadata(0)
		var preview := Label.new()
		preview.text = item.get_text(0)
		set_drag_preview(preview)
		return {"kind":"kokoverse_reference","reference":{"kind":"slot","target":controller.manager.app.address_of(controller.manager.app.objects[id])}}
	func _can_drop_data(at: Vector2, data: Variant) -> bool:
		return data is Dictionary and data.get("kind") == "kokoverse_reference" and data.get("reference",{}).get("kind") == "slot" and get_item_at_position(at) != null
	func _drop_data(at: Vector2, data: Variant) -> void:
		var item := get_item_at_position(at)
		controller.move_object(data.reference.target.instance_id, item.get_metadata(0))

var inspector
var manager
var tree := SlotTree.new()
var root_label := Label.new()
var filling := false
var signature := ""
var tree_state: Dictionary = {}
var child_button: Button

func setup(view) -> void:
	inspector = view
	manager = view.manager
	custom_minimum_size.x = 210
	root_label.add_theme_font_size_override("font_size", 18)
	root_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	add_child(root_label)
	var navigation := HBoxContainer.new()
	add_child(navigation)
	view.add_button(navigation,"World",func(): set_root(null))
	view.add_button(navigation,"↑",func():
		var root: Variant = tree_state.get("root")
		set_root(manager.app.objects.get(root.instance_id,{}).get("parent")) if root != null else set_root(null))
	view.add_button(navigation,"Focus",func():
		if manager.app.objects.has(view.target_id): set_root(manager.app.address_of(manager.app.objects[view.target_id])))
	tree.controller = self
	tree.hide_root = false
	tree.size_flags_vertical = Control.SIZE_EXPAND_FILL
	tree.add_theme_font_size_override("font_size", 20)
	add_child(tree)
	tree.item_selected.connect(func():
		if filling: return
		var id: Variant = tree.get_selected().get_metadata(0)
		if id != null and manager.app.objects.has(id):
			inspector.choose_target(str(id))
			manager.app.select_object(str(id)))
	tree.item_collapsed.connect(func(item: TreeItem):
		if filling or item.get_metadata(0) == null: return
		var next := tree_state.duplicate(true)
		var address: Dictionary = manager.app.address_of(manager.app.objects[item.get_metadata(0)])
		next.collapsed.erase(address)
		if item.collapsed: next.collapsed.append(address)
		manager.edit(inspector.panel_id,"inspector_tree",next))
	child_button = view.add_button(self,"+ Child",func():
		if manager.app.objects.has(view.target_id):
			manager.command({"kind":"structure","command":{"action":"create_child","parent":manager.app.address_of(manager.app.objects[view.target_id])}},func(result):
				if result.ok: inspector.choose_target(result.selected.instance_id)
				else: inspector.message.text = result.get("message","Could not create child")))
	view.add_button(self,"Move to world",func(): move_object(view.target_id,null))
	view.add_button(self,"Duplicate",func():
		var object: Dictionary = manager.app.objects.get(view.target_id,{})
		if not object.is_empty():
			manager.command({"kind":"structure","command":{"action":"duplicate","target":manager.app.address_of(object),"expected_revision":object.object_revision}},func(result):
				if result.ok: inspector.choose_target(result.selected.instance_id)
				else: inspector.message.text = result.get("message","Could not duplicate")))

func set_root(address: Variant) -> void:
	var next := tree_state.duplicate(true)
	next["root"] = address
	manager.edit(inspector.panel_id,"inspector_tree",next)

func move_object(id: String, parent_id: Variant) -> void:
	var object: Dictionary = manager.app.objects.get(id,{})
	if object.is_empty(): return
	var parent: Variant = manager.app.address_of(manager.app.objects[parent_id]) if parent_id != null and manager.app.objects.has(parent_id) else null
	manager.command({"kind":"structure","command":{"action":"reparent","target":manager.app.address_of(object),"parent":parent,"expected_revision":object.object_revision}},func(result):
		if not result.ok: inspector.message.text = result.get("message","Could not move object"))

func refresh() -> void:
	tree_state = inspector.panel_state.inspector.get("tree",{"root":null,"collapsed":[]})
	var entries: Array = []
	for id in manager.app.objects:
		entries.append([id,manager.app.objects[id].get("parent"),manager.field_data(id,"name").get("value",id)])
	var next := JSON.stringify([entries,tree_state,inspector.target_id])
	if next == signature: return
	signature = next
	filling = true
	tree.clear()
	var root := tree.create_item()
	var address: Variant = tree_state.get("root")
	if address != null and manager.app.objects.has(address.instance_id):
		root.set_metadata(0,address.instance_id)
		root.set_text(0,str(manager.field_data(address.instance_id,"name").get("value",address.instance_id)))
		add_children(root,address.instance_id,0)
	else:
		root.set_text(0,"World")
		add_children(root,"",0)
	root_label.text = "Root: " + root.get_text(0)
	filling = false

func add_children(parent: TreeItem, parent_id: String, depth: int) -> void:
	if depth > 128: return
	for id in manager.app.objects:
		var object: Dictionary = manager.app.objects[id]
		var address: Variant = object.get("parent")
		if (address == null and parent_id != "") or (address != null and address.instance_id != parent_id): continue
		var item := tree.create_item(parent)
		item.set_metadata(0,id)
		item.set_text(0,str(manager.field_data(id,"name").get("value",id)))
		item.set_tooltip_text(0,"Drag onto another Slot to reparent; drop on World to detach")
		item.collapsed = tree_state.get("collapsed",[]).has(manager.app.address_of(object))
		if id == inspector.target_id: item.select(0)
		add_children(item,id,depth+1)
