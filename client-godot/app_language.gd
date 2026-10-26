extends Node
## Display language belongs to this client, never to the shared world.
signal language_changed

const LOCALES := ["ja", "ko"]
const CATALOGS := ["res://locale/ko_shell.json", "res://locale/ko_services.json", "res://locale/ko_tools.json"]
var locale := "ja"
var settings_path := "user://interface.cfg"
var bindings := {}

func _ready() -> void:
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--language-settings="):
			settings_path = argument.trim_prefix("--language-settings=")
	var korean := Translation.new()
	korean.locale = "ko"
	for path in CATALOGS:
		var messages: Variant = JSON.parse_string(FileAccess.get_file_as_string(path))
		if not messages is Dictionary:
			push_error("Invalid translation catalog: " + path)
			continue
		for source in messages:
			korean.add_message(source, messages[source])
	TranslationServer.add_translation(korean)
	load_preference()
	# Windows' system fallback includes Japanese and Hangul in Controls and 3D labels.
	var font := SystemFont.new()
	font.font_names = PackedStringArray(["Segoe UI", "Yu Gothic UI", "Malgun Gothic"])
	ThemeDB.fallback_font = font

func load_preference() -> String:
	var config := ConfigFile.new()
	var saved: Variant = "ja"
	if config.load(settings_path) == OK:
		saved = config.get_value("interface", "language", "ja")
	locale = saved if saved is String and saved in LOCALES else "ja"
	apply_locale()
	return locale

func set_locale(code: String) -> bool:
	if code not in LOCALES: return false
	locale = code
	apply_locale()
	var config := ConfigFile.new()
	config.load(settings_path)
	config.set_value("interface", "language", locale)
	return config.save(settings_path) == OK

func apply_locale() -> void:
	TranslationServer.set_locale(locale)
	for key in bindings.keys():
		var binding: Dictionary = bindings[key]
		var control = binding.node.get_ref()
		# A later status message supersedes its previous formatted binding.
		if control == null or control.get(binding.property) != binding.rendered:
			bindings.erase(key)
		else:
			render_binding(binding, control)
	language_changed.emit()

func format_text(control: Node, source: String, values: Array = [], property: String = "text") -> void:
	var binding := {"node":weakref(control), "source":source, "values":values.duplicate(), "property":property, "rendered":""}
	var key := str(control.get_instance_id()) + ":" + property
	if not bindings.has(key):
		control.tree_exiting.connect(func(): bindings.erase(key), CONNECT_ONE_SHOT)
	bindings[key] = binding
	render_binding(binding, control)

func render_binding(binding: Dictionary, control: Node) -> void:
	var translated := str(TranslationServer.translate(binding.source))
	var rendered: String = translated % binding.values if not binding.values.is_empty() else translated
	control.set(binding.property, rendered)
	binding.rendered = rendered
