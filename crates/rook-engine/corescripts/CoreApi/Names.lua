-- Every SetCore/GetCore name Rook's CoreScripts answer, the piece that
-- answers it, and where its behavior comes from. A piece registers only
-- the names listed for it here (CoreApi.register checks).
return {
	TopbarEnabled = {
		owner = "TopBar",
		-- creator-docs StarterGui:SetCore: "TopbarEnabled" hides the top
		-- bar; the menu still opens with Escape.
		cite = "creator-docs StarterGui:SetCore",
	},
}
