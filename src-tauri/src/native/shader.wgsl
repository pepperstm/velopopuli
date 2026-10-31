// The native renderer's shaders: Lambert lighting with a hemisphere sky light, a sun
// and a lamp, linear-distance fog, a gradient sky, instanced trees and text billboards.
// Light levels follow three.js (diffuse = colour × irradiance / π) so both renderers match.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    hemi_sky: vec4<f32>,
    hemi_ground: vec4<f32>,
    fog_color: vec4<f32>,
    // near, far
    fog: vec4<f32>,
    // xyz position, w intensity
    lamp: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    // x: 0 none, 1 rain, 2 snow; y: how far everything has fallen (0…1 of the box);
    // zw: how far it has streamed back past the rider (0…1 of the box)
    weather: vec4<f32>,
    // x: seconds, wrapping
    clock: vec4<f32>,
};

struct Draw {
    model: mat4x4<f32>,
    tint: vec4<f32>,
    // x: lit (1) or unlit (0); y: jersey mode (colour + uv.x × tint); z: flat shading; w: plain white (ghosts)
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var<uniform> d: Draw;

const PI: f32 = 3.14159265;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) uv: vec2<f32>) -> VsOut {
    var o: VsOut;
    let world = d.model * vec4<f32>(pos, 1.0);
    o.world = world.xyz;
    o.normal = (d.model * vec4<f32>(normal, 0.0)).xyz;
    o.color = color;
    o.uv = uv;
    o.clip = g.view_proj * world;
    return o;
}

/// A tree: its kind's shared mesh placed by `[x, y, z, yaw, width, height, kind, shade]`.
@vertex
fn vs_tree(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) uv: vec2<f32>,
           @location(4) at: vec3<f32>, @location(5) shape: vec3<f32>, @location(6) kind_shade: vec2<f32>) -> VsOut {
    var o: VsOut;
    let c = cos(shape.x);
    let s = sin(shape.x);
    let p = vec3<f32>(pos.x * shape.y, pos.y * shape.z, pos.z * shape.y);
    let r = vec3<f32>(c * p.x + s * p.z, p.y, -s * p.x + c * p.z);
    let world = r + at + d.model[3].xyz;
    o.world = world;
    o.normal = vec3<f32>(c * normal.x + s * normal.z, normal.y, -s * normal.x + c * normal.z);
    o.color = color * kind_shade.y;
    o.uv = uv;
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    // Beyond its switching distance the tree is drawn as a sprite instead.
    let base = at + d.model[3].xyz;
    if (distance(base.xz, g.camera.xz) > sprite_from(base)) {
        o.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return o;
}

/// Each tree's own switching distance from 3D to sprite, from where it stands. As
/// `spriteFrom` in the page's `trees.ts`, and `SPRITES_FROM` / `SPRITE_SPREAD` in the core.
fn sprite_from(at: vec3<f32>) -> f32 {
    return 200.0 + (fract(sin(dot(at.xz, vec2<f32>(12.9898, 78.233))) * 43758.5453) - 0.5) * 30.0;
}

/// A tree as a sprite: its kind's quad (`pos.x` across, `pos.y` up) standing at the tree,
/// turned about the vertical to face the camera, scaled like the tree; lit as if facing the
/// camera and a little upwards, much as a crown's surfaces are on average.
@vertex
fn vs_sprite(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) uv: vec2<f32>,
             @location(4) at: vec3<f32>, @location(5) shape: vec3<f32>, @location(6) kind_shade: vec2<f32>) -> VsOut {
    var o: VsOut;
    let base = at + d.model[3].xyz;
    let to_camera = g.camera.xz - base.xz;
    let facing = normalize(to_camera + vec2<f32>(0.0001, 0.0));
    let right = vec3<f32>(facing.y, 0.0, -facing.x);
    let world = base + right * pos.x * shape.y + vec3<f32>(0.0, pos.y * shape.z, 0.0);
    o.world = world;
    o.normal = normalize(vec3<f32>(facing.x * 0.6, 0.8, facing.y * 0.6));
    o.color = color * kind_shade.y;
    o.uv = uv;
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    if (length(to_camera) <= sprite_from(base)) {
        o.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return o;
}

/// A race barrier section: one shared model placed by `[x, y, z, yaw, pitch, -]`, tipped
/// about its own x axis to lie along the slope, then turned to follow the road.
@vertex
fn vs_barrier(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>, @location(3) uv: vec2<f32>,
              @location(4) at: vec3<f32>, @location(5) shape: vec3<f32>) -> VsOut {
    var o: VsOut;
    let cp = cos(shape.y);
    let sp = sin(shape.y);
    let p = vec3<f32>(pos.x, cp * pos.y - sp * pos.z, sp * pos.y + cp * pos.z);
    let n = vec3<f32>(normal.x, cp * normal.y - sp * normal.z, sp * normal.y + cp * normal.z);
    let c = cos(shape.x);
    let s = sin(shape.x);
    let world = vec3<f32>(c * p.x + s * p.z, p.y, -s * p.x + c * p.z) + at + d.model[3].xyz;
    o.world = world;
    o.normal = vec3<f32>(c * n.x + s * n.z, n.y, -s * n.x + c * n.z);
    o.color = color;
    o.uv = uv;
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    return o;
}

fn fogged(c: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let dist = distance(world, g.camera.xyz);
    let f = smoothstep(g.fog.x, g.fog.y, dist);
    return mix(c, g.fog_color.rgb, f);
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    return shade(i, textureSample(tex, samp, i.uv));
}

/// A sprite: only where its picture is.
@fragment
fn fs_sprite(i: VsOut) -> @location(0) vec4<f32> {
    let texel = textureSample(tex, samp, i.uv);
    if (texel.a < 0.5) {
        discard;
    }
    return shade(i, vec4<f32>(texel.rgb, 1.0));
}

fn shade(i: VsOut, texel: vec4<f32>) -> vec4<f32> {
    var base = i.color * d.tint.rgb * texel.rgb;
    if (d.flags.y > 0.5) {
        // Rider models: the jersey's share of each vertex takes the rider's colour.
        base = i.color + i.uv.x * d.tint.rgb;
    }
    if (d.flags.w > 0.5) {
        base = vec3<f32>(1.0);
    }
    var lit = base;
    if (d.flags.x > 0.5) {
        var n = normalize(i.normal);
        if (d.flags.z > 0.5) {
            // Flat shading, facing the viewer (the camera is at the origin).
            n = normalize(cross(dpdx(i.world), dpdy(i.world)));
            if (dot(n, -i.world) < 0.0) { n = -n; }
        }
        let hemi = mix(g.hemi_ground.rgb, g.hemi_sky.rgb, 0.5 * n.y + 0.5);
        let sun = g.sun_color.rgb * max(dot(n, g.sun_dir.xyz), 0.0);
        let to_lamp = g.lamp.xyz - i.world;
        let ld = length(to_lamp);
        let falloff = 1.0 / max(pow(ld, 1.6), 0.01) * pow(clamp(1.0 - pow(ld / 32.0, 4.0), 0.0, 1.0), 2.0);
        let lamp = vec3<f32>(1.0, 0.957, 0.878) * g.lamp.w * falloff * max(dot(n, to_lamp / max(ld, 0.001)), 0.0);
        lit = base * (hemi + sun + lamp) / PI;
    }
    return vec4<f32>(fogged(lit, i.world), d.tint.a * texel.a);
}

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) v: u32) -> SkyOut {
    var o: SkyOut;
    let xy = vec2<f32>(f32((v << 1u) & 2u), f32(v & 2u)) * 2.0 - 1.0;
    o.clip = vec4<f32>(xy, 0.0, 1.0);
    o.ndc = xy;
    return o;
}

@fragment
fn fs_sky(i: SkyOut) -> @location(0) vec4<f32> {
    let far = g.inv_view_proj * vec4<f32>(i.ndc, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w - g.camera.xyz);
    let h = clamp(dir.y, 0.0, 1.0);
    return vec4<f32>(mix(g.sky_bottom.rgb, g.sky_top.rgb, pow(h, 0.55)), 1.0);
}

struct LabelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
};

/// A camera-facing text sign: centre, corner (−1…1), size in metres and atlas coordinates.
@vertex
fn vs_label(@location(0) centre: vec3<f32>, @location(1) corner: vec2<f32>, @location(2) size: vec2<f32>, @location(3) uv: vec2<f32>) -> LabelOut {
    var o: LabelOut;
    let world = centre + g.cam_right.xyz * corner.x * size.x * 0.5 + g.cam_up.xyz * corner.y * size.y * 0.5;
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    o.uv = uv;
    o.world = world;
    return o;
}

@fragment
fn fs_label(i: LabelOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    if (c.a < 0.02) { discard; }
    return vec4<f32>(fogged(c.rgb, i.world), c.a);
}

/// A rider's soft shadow: a blurred disc, as the page draws it (radial falloff 1 → 0.6 → 0).
@fragment
fn fs_shadow(i: VsOut) -> @location(0) vec4<f32> {
    let r = length(i.uv * 2.0 - 1.0);
    let a = select(mix(0.604, 0.0, (r - 0.45) / 0.55), mix(1.0, 0.604, r / 0.45), r < 0.45);
    return vec4<f32>(fogged(vec3<f32>(0.0), i.world), 0.42 * clamp(a, 0.0, 1.0));
}

// ── Rain and snow ─────────────────────────────────────────────────────────────
// Each drop or flake has a fixed random place in a box around the camera; the box
// content falls and streams back as the uniforms move on, so nothing is updated per frame.

const BOX: f32 = 70.0;
const BOX_HEIGHT: f32 = 40.0;

fn hash(n: u32) -> u32 {
    var x = n * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn rand3(i: u32) -> vec3<f32> {
    let a = hash(i);
    let b = hash(a);
    let c = hash(b);
    return vec3<f32>(f32(a), f32(b), f32(c)) / 4294967296.0;
}

/// Where particle `i` is now, relative to the camera.
fn particle(i: u32) -> vec3<f32> {
    let r = rand3(i);
    var sway = 0.0;
    if (g.weather.x > 1.5) {
        sway = 0.4 * sin(g.clock.x * 0.7 + f32(i));
    }
    let x = fract(r.x + g.weather.z + sway / BOX) - 0.5;
    let z = fract(r.z + g.weather.w) - 0.5;
    let y = fract(r.y - g.weather.y) - 0.5;
    return vec3<f32>(x * BOX, y * BOX_HEIGHT, z * BOX);
}

struct PrecipOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

/// Rain: a short streak per drop (line list, two vertices each).
@vertex
fn vs_rain(@builtin(vertex_index) v: u32) -> PrecipOut {
    var o: PrecipOut;
    let p = particle(v / 2u) + vec3<f32>(0.02, 0.7, 0.0) * f32(v & 1u);
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.uv = vec2<f32>(0.0);
    return o;
}

@fragment
fn fs_rain(i: PrecipOut) -> @location(0) vec4<f32> {
    // #aab8c8 in linear light, mostly see-through.
    return vec4<f32>(0.402, 0.479, 0.578, 0.45);
}

/// Snow: a small camera-facing disc per flake (six vertices each).
@vertex
fn vs_snow(@builtin(vertex_index) v: u32) -> PrecipOut {
    var o: PrecipOut;
    let corners = array<vec2<f32>, 6>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));
    let c = corners[v % 6u];
    let p = particle(v / 6u) + (g.cam_right.xyz * c.x + g.cam_up.xyz * c.y) * 0.07;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.uv = c;
    return o;
}

@fragment
fn fs_snow(i: PrecipOut) -> @location(0) vec4<f32> {
    if (dot(i.uv, i.uv) > 1.0) { discard; }
    return vec4<f32>(1.0, 1.0, 1.0, 0.9);
}
