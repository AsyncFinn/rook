# M11 Avatars and animation

How players look and move. BodyColors 359 files, Shirt 330, Hat 323,
Pants 282, CharacterMesh 168, ShirtGraphic 89, Accessory 42.

1. A local avatar (body colors, clothing, hats and faces from content
   folders or the user's install), set up without a Roblox account and
   sent to the server on joining.
2. Clothing and ShirtGraphics on R15 and R6 bodies, CharacterMeshes,
   hats and accessories, as Studio draws them; R15 body parts from
   HumanoidDescriptions (M9 builds the default ones), and the
   AnimationConstraint joints Studio's rigs use where M9's have
   Motor6Ds.
3. Layered clothing (WrapLayer, WrapTarget: 12 files) and dynamic heads
   (FaceControls).
4. Animation beyond what M9's Animate scripts need: AnimationController
   on rigs without a Humanoid, IKControl, and tracks replicated between
   players.
