# Tags and locked members

Checked against Studio 0.746 on 2026-11-06, in conformance cases
`signals/tag_signals` and `instance/locked_member_children`. The Studio
runs were in Edit mode at plugin identity, because playtests wouldn't
start, with the deferred signal behavior the expected files name.

Code: the `TagPresence` journal entry in `rook-dom`, `SignalKind::Tag` in
`rook-lua`.

## Tag signals

`CollectionService:GetInstanceAddedSignal(tag)` and
`GetInstanceRemovedSignal(tag)`:

- Fire when a tagged instance enters or leaves the DataModel, or its tag
  changes while it's inside. Tagging outside the game fires nothing, nor
  does moving within it, nor adding a tag it has or removing one it lacks.
- A subtree entering or leaving fires for each tagged instance, in
  pre-order. `Destroy` fires removed.
- Print as `Signal <tag>InstanceAdded` and `Signal <tag>InstanceRemoved`.
- A number converts to its text; no argument errors
  `Argument 1 missing or nil`; an empty tag errors
  `Empty tag string provided to GetInstanceAddedSignal` (or Removed).

Two reads of one signal compare equal, for these and for `ChildAdded`,
`GetPropertyChangedSignal` and `GetAttributeChangedSignal`.

## Locked members

What only the engine reaches (RobloxSecurity or NotAccessibleSecurity to
read) isn't a member to a script, so a child of that name is found, and
with no child the error is `X is not a valid member of Folder "Holder"`.
That covers `Tags`, `Attributes`, `HistoryId` and others a place's child
might be named. A write fails the same way.

RobloxScriptSecurity members are still members, and beat a child:

- Reading errors `The current thread cannot read 'X' (lacking capability
  RobloxScript)`.
- Writing errors `cannot write`, naming the capability reading takes when
  it can't be read, whatever writing takes (`UniqueId`).

`Instance.Capabilities` is a real property in Studio, of a datatype Rook
doesn't have yet, so a child of that name is found in Rook and isn't in
Studio.
