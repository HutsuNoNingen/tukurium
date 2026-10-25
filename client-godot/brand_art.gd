extends RefCounted
## Product artwork supplied for tukurium. Atlas regions trim transparent margins at display time.
const LOGO = preload("res://assets/branding/logo.png")
const MASCOT = preload("res://assets/branding/mascot.png")

static func crop(source: Texture2D, area: Rect2) -> AtlasTexture:
	var result := AtlasTexture.new()
	result.atlas = source
	result.region = area
	return result

static func logo() -> AtlasTexture:
	return crop(LOGO, Rect2(190, 75, 1160, 885))

static func wordmark() -> AtlasTexture:
	return crop(LOGO, Rect2(190, 748, 1160, 210))

static func mascot() -> AtlasTexture:
	return crop(MASCOT, Rect2(125, 170, 1070, 925))

static func picture(source: Texture2D, size: Vector2) -> TextureRect:
	var result := TextureRect.new()
	result.texture = source
	result.custom_minimum_size = size
	result.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	result.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	result.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS
	result.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return result
