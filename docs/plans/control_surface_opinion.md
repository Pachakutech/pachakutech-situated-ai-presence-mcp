# Opinion on the control surface

This is a reading of `docs/plans/control_surface.md`, not a change to it and not a ticket. The architecture is the right one. It is not finished, and a few of its easy sentences are ahead of the binary.

## What is strong

The cut is in the right place. A pose, a yaw, and a seat are a presence. Fifty thousand splats are a picture of one. Sitting teacher already wants the first list. The CPU cost note already worries about the second. Putting them in one record means IK, a clip, and a later model all write the same small struct, and the GPU is the only thing that wears it.

That matches the pattern worth copying from the work shown that week: a short control a person can still read, and a large elaboration that is allowed to be beautiful. We do not have to train a model to get the benefit. The record is what a model would be trained to emit, later, if we ever want one. Until then it is what the mixer emits, and that is enough to ship.

The bake is a real gate, not a slogan. `scripts/bake_splatbind.py` refuses a cloud that is not the mesh it claims. Customizability can be advertised because a guest body has a concrete admission test: rigged mesh, cloud through that script, fingerprint matches. The default stays the unlabeled body, skinned from the room. Guests bring their own color. The embellisher is a mode of the glass, not a second avatar stack. That is a small promise and it is honest.

The heat field stays out of the body. Two integers join the record. The grid stays shared memory for later actors. That is the No Mind shape, and it keeps placement from becoming a private subroutine of the skinning pass.

The projection contract survives. Guest clouds and the default both arrive as finished splats. `gpu_layout.rs` does not grow a special case per client.

## What is weak

The proud path is not the one running. Every 33 ms the CPU still skins the mesh and uploads the whole cloud. The record is a document. The expand is the binary. Sitting teacher will make that louder, because a held gesture is the same pose thirty times a second. The skip while the record is unchanged is the first repair, and it should land with the mixer, not after a year of clips.

A guest rig is not our skeleton. The bake proves a cloud belongs to its own mesh. Lotus, `talk_point`, the jaw, and any IK we write are authored for the default joints. Advertising "bring your avatar" before a retarget into those joint locals, or a clear rule that a guest brings its own clips and gets no lotus, will produce bodies that load and then cannot sit. The embellisher does not solve that. It only solves color.

Two weights in one pass can still rot. If the default's environment skin and the guest's preserved albedo grow separate shaders, we will have two bodies in practice. The discipline is one entry point and a weight. The weakness is that discipline is social. Nothing in the types enforces it yet.

The default's look is the art, and it is also easy to misunderstand. A figure skinned from the room is situated, and it is not a character with a wardrobe. If it remains the only body for a long time, the custom path is a brochure. That is acceptable if we say so. It is a problem if a client arrives with a cloud and the only code that can draw is the environment lottery.

IK can leak vertices. The moment a solver is easier to write as "output a posed mesh," the control surface splits. The opinionated rule is that IK ends in joint locals. The weakness is that a reaching arm wants a hand target, and the temptation to cache the posed vertices will be strong the first time the CPU deform is the only consumer.

Logging the record is claimed and not done. Without that log, the sentence about a later model training on our poses is a wish. The strength does not depend on the model. It does depend on the record existing in code before the clips multiply.

Color as a labeled control is a different product, closer to AR. Leaving it out is correct. The risk is smuggling it back as "region intents" on the record because the table looked empty. The room, and a guest's baked color, already cover what the screen needs to show.

## Where I would spend the next attention

Name the record in the mixer, and skip the upload when it has not changed. That is the whole near-term architecture, and it is what makes sitting teacher cheap enough to finish. The embellisher is a note on the glass, written down so a supplied cloud has a place to land, and not built until a second body exists. The GPU deform waits until a clip is actually playing every frame. The retarget for guest skeletons waits until someone brings a mesh that is not ours.
