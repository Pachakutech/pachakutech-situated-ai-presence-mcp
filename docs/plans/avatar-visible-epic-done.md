# Avatar visible epic — done

The 220×220 pass-through layer-shell surface shows the default avatar. While that cloud is bound, the screen-capture quad is not drawn. Transparent pixels around the body show the desktop. The input region stays pass-through, and the surface stays 220×220. Other avatars later use the same manifest pointer.

Speech (Piper, Rhubarb, `faceanim`, MCP `avatar.speak`) is a later epic. Stop here.

## Run

```
cargo build --release --manifest-path daemon/Cargo.toml
./daemon/target/release/presence-daemon
```

Manifest search, first hit wins:

1. `PRESENCE_AVATAR_MANIFEST`, when set
2. `./assets/avatar_manifest.json`
3. `../assets/avatar_manifest.json`
4. `daemon/../assets/avatar_manifest.json` (the path compiled into the binary)

The checked-in manifest only names the mesh, the splatbind, and the jaw. The GLB owns the skeleton.

```
assets/avatar_manifest.json
  asset:    humanoid_proxy_rigged.glb
  splatbind: humanoid_proxy_rigged.splatbind
  jaw_joint: jaw
```

Socket: `$XDG_RUNTIME_DIR/pachakutech/presence.sock`, or `PRESENCE_DAEMON_SOCKET` for both the daemon and the `presence` CLI.

Startup prints the fingerprint, splat count (50000), joint count (160), and the jaw name. The first projection must write at least 1000 live discs or the process exits. That check needs a GPU. Unit tests do not:

```
cargo test --manifest-path daemon/Cargo.toml --bin presence-daemon -- avatar::
```

There is no library target. `cargo test --lib` does not apply.

Before the avatar loads, a one-splat identity-camera smoke must project its probe. That is the non-rigged path: bone 0 stays identity, projection is still a pure dual quaternion, and `gpu_layout.rs` is unchanged. The avatar writes finished positions with `skin = None` into the reserved slot range above the 4096 artifact slots.

## What moves

This GLB has no animation clips. `play("idle")` and `play("walk")` leave the skeleton in the rest pose. With no socket command, the demo sways root yaw and opens the jaw on a 33 ms tick. `walk_to` turns and slides the root on the ground plane. It does not play a walk cycle. Discs are isotropic.

Capture stays about 10 fps and only feeds the screen quad. That quad is skipped while the avatar range is live, so the present the tick drives is the body.

## Controls

One JSON object per line on the socket:

```
{"kind":"avatarRest","proposalId":"rest"}
{"kind":"avatarFace","proposalId":"jaw","jawOpen":1}
{"kind":"avatarFace","proposalId":"morph","jawOpen":0,"morphIndex":0,"morphWeight":1}
{"kind":"avatarWalk","proposalId":"walk","x":1,"z":0}
{"kind":"shutdown","proposalId":"stop"}
```

The same commands through the CLI, with `PRESENCE_DAEMON_SOCKET` pointed at the daemon you started:

```
presence avatar rest
presence avatar jaw 1
presence avatar morph 0 1
presence avatar walk 1 0
```

Rest, jaw, morph, and walk turn the demo sway off. Rest also closes the jaw and clears a walk target.

## Device check

On this machine (Hyprland, 1920×1080, the layer at 850,430, Intel Iris Xe) the release daemon logged `50000 discs in the layer`. A center crop of the layer is an upright colored body, head toward the top of the surface. `avatarRest` holds that pose. `jawOpen: 1` opens the mouth and leaves the torso and legs where they were.
