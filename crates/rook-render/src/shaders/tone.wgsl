// Studio's frame and tone curve (docs/behavior/lighting/tone-mapping.md). Studio
// draws light units (Ambient's color squared, a light's Brightness x Color
// squared) into a frame holding sqrt(light / 4), so 4 units is the
// brightest it keeps, and blends Decals, Transparency and particles in that
// encoding (Confirmed: the composition shaders in `shaders_glsl3.pack`
// decode the frame as 4 x value squared). One curve at the end turns light
// units into the screen's pixels.

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3(0.0031308));
}

// A surface color as Studio's shaders take it: they square the sRGB
// color (texture times color), where Rook's colors and images arrive as
// linear light (`shaders_glsl3.pack`, 0.743; within a level of the product
// of the squares except near black).
fn albedo_units(linear: vec3<f32>) -> vec3<f32> {
    let c = linear_to_srgb(max(linear, vec3(0.0)));
    return c * c;
}

// The brightest light the frame keeps, in light units.
const FRAME_LIGHT: f32 = 4.0;

// The default tone curve (`ColorGradingEffect.TonemapperPreset` Default, and
// places without one): light units to the linear light an sRGB target turns
// into Studio's pixels. Measured on white SmoothPlastic: Ambient 32, 64, 96,
// 128, 160, 192 and 255 gray drew 14, 47, 91, 134, 166, 193 and 224, brighter
// lights up to 253; black Decals over Neon at each Transparency land on it
// within 4 levels.
const CURVE_IN = array<f32, 11>(0.0, 0.0156, 0.0625, 0.141, 0.25, 0.393, 0.567, 1.0, 2.24, 3.5, 8.0);
const CURVE_OUT = array<f32, 11>(0.0, 0.00437, 0.0284, 0.104, 0.239, 0.381, 0.533, 0.745, 0.939, 0.982, 1.0);

// Piecewise linear through (`xs`, `ys`), flat past the last point.
fn through(v: f32, xs: array<f32, 11>, ys: array<f32, 11>) -> f32 {
    if v >= xs[10] {
        return ys[10];
    }
    var i = 0;
    while i < 9 && v > xs[i + 1] {
        i++;
    }
    return mix(ys[i], ys[i + 1], clamp((v - xs[i]) / (xs[i + 1] - xs[i]), 0.0, 1.0));
}

fn studio_light(v: f32) -> f32 {
    return through(v, CURVE_IN, CURVE_OUT);
}

fn studio_light3(v: vec3<f32>) -> vec3<f32> {
    return vec3(studio_light(v.x), studio_light(v.y), studio_light(v.z));
}

// The light units the default curve draws as linear light `v`: how Rook's
// looks fitted to Studio's pixels reach the frame.
fn light_units(v: f32) -> f32 {
    return through(v, CURVE_OUT, CURVE_IN);
}

fn light_units3(v: vec3<f32>) -> vec3<f32> {
    return vec3(light_units(v.x), light_units(v.y), light_units(v.z));
}

// Light units as the frame holds them.
fn frame(units: vec3<f32>) -> vec3<f32> {
    return sqrt(clamp(units / FRAME_LIGHT, vec3(0.0), vec3(1.0)));
}

// A look fitted in linear light (what Studio's pixels are under the default
// curve) as the frame holds it.
fn frame_of(linear: vec3<f32>) -> vec3<f32> {
    return frame(light_units3(linear));
}
