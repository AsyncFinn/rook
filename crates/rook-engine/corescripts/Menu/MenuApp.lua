-- The menu as Roblox's lays it out (Studio 0.748; Components.Theme has
-- the measurements): DarkenBackground over the game, and SettingsShield,
-- a button the size of the screen that keeps clicks from the game,
-- holding the panel (MenuContainer) with the tabs (HubBar), the page
-- (PageViewClipper) and Leave, Respawn and Resume (BottomButtonFrame),
-- or the Leave confirmation in their place.
--
-- Opening, the shield slides up from below the screen (Quart InOut) as
-- the backdrop fades in (Quad Out), both over 0.5 s. Closing, the
-- backdrop goes at once and the shield slides back down (Quad In,
-- 0.4 s); games hear the menu closed when it's gone.

local CorePackages = game:GetService("CorePackages")
local TweenService = game:GetService("TweenService")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent
local Components = require(Modules.Components)
local Theme = Components.Theme
local Menu = require(script.Parent)
local LeaveConfirm = require(script.Parent.LeaveConfirm)
local Pages = require(script.Parent.Pages)

local e = React.createElement
local M = Theme.menu

-- The window's size, following it.
local function useViewport(): Vector2
	local camera = workspace.CurrentCamera
	local size, setSize = React.useState(if camera then camera.ViewportSize else Vector2.new(1280, 720))
	React.useEffect(function()
		local cam = workspace.CurrentCamera
		if not cam then
			return nil
		end
		setSize(cam.ViewportSize)
		local c = cam:GetPropertyChangedSignal("ViewportSize"):Connect(function()
			setSize(cam.ViewportSize)
		end)
		return function()
			c:Disconnect()
		end
	end, {})
	return size
end

local function BottomButtons(props: { height: number, top: number })
	local count, gap = 3, Theme.button.gap
	local function button(i, p)
		p.AnchorPoint = Vector2.new(0, 0.5)
		p.Position = UDim2.new((i - 1) / count, (i - 1) * gap / count, 0.5, 0)
		p.Size = UDim2.new(1 / count, -gap * (count - 1) / count, 0, Theme.button.height)
		return e(Components.Button, p)
	end
	return e("Frame", {
		BackgroundTransparency = 1,
		Position = UDim2.fromOffset(M.padding, props.top),
		Size = UDim2.new(1, -2 * M.padding, 0, props.height),
	}, {
		LeaveButton = button(1, {
			text = "Leave",
			hint = "L",
			onActivated = function()
				Menu.setLeaving(true)
			end,
		}),
		-- Rook has no death to respawn from yet (docs/behavior/gui/menu.md).
		RespawnButton = button(2, { text = "Respawn", hint = "R", disabled = true }),
		ResumeButton = button(3, {
			text = "Resume",
			hint = "Esc",
			primary = true,
			onActivated = function()
				Menu.setOpen(false)
			end,
		}),
	})
end

return function()
	local state = Menu.useState()
	local viewport = useViewport()
	local small = viewport.X < M.smallWidth
	local shieldRef = React.useRef(nil)
	local darkenRef = React.useRef(nil)

	React.useEffect(function()
		local shield, darken = shieldRef.current, darkenRef.current
		if not (shield and darken) then
			return nil
		end
		local tweens = {}
		if state.open then
			shield.Visible = true
			shield.Position = M.offscreen
			darken.Visible = true
			darken.BackgroundTransparency = 1
			local slide = TweenInfo.new(M.openSeconds, Enum.EasingStyle.Quart, Enum.EasingDirection.InOut)
			local fade = TweenInfo.new(M.openSeconds, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
			tweens[1] = TweenService:Create(shield, slide, { Position = UDim2.new() })
			tweens[2] = TweenService:Create(darken, fade, { BackgroundTransparency = M.backdropTransparency })
		elseif shield.Visible then
			darken.Visible = false
			local slide = TweenInfo.new(M.closeSeconds, Enum.EasingStyle.Quad, Enum.EasingDirection.In)
			tweens[1] = TweenService:Create(shield, slide, { Position = M.offscreen })
			tweens[1].Completed:Connect(function(how)
				if how == Enum.PlaybackState.Completed then
					shield.Visible = false
					Menu.closed()
				end
			end)
		end
		for _, tween in tweens do
			tween:Play()
		end
		return function()
			for _, tween in tweens do
				tween:Cancel()
			end
		end
	end, { state.open })

	local top = if small then M.smallTabBarHeight else M.tabBarHeight
	local pageHeight = if small then viewport.Y - M.smallPageInset else M.pageHeight
	local bottom = if small then M.smallBottomHeight else M.bottomHeight
	local width = if small then viewport.X else M.panelSize.X
	local height = top + pageHeight + bottom + M.paddingBottom
	local page = Pages[state.page] or Pages.Settings

	return e(React.Fragment, nil, {
		-- A button, so the game's buttons behind it get no clicks while
		-- the shield is still sliding in: the topmost button under the
		-- cursor takes a click whatever covers it (docs/behavior/gui/input.md).
		DarkenBackground = e("ImageButton", {
			AutoButtonColor = false,
			Visible = false,
			BorderSizePixel = 0,
			BackgroundColor3 = Theme.color.black,
			BackgroundTransparency = 1,
			Size = UDim2.fromScale(1, 1),
			ref = darkenRef,
		}),
		SettingsShield = e("ImageButton", {
			AutoButtonColor = false,
			Visible = false,
			BackgroundTransparency = 1,
			Position = M.offscreen,
			Size = UDim2.fromScale(1, 1),
			ZIndex = 2,
			ref = shieldRef,
		}, {
			-- Frees the mouse while the menu is open.
			Modal = e("TextButton", {
				Modal = true,
				Text = "",
				BackgroundTransparency = 1,
				Size = UDim2.fromScale(1, 1),
			}),
			MenuContainer = e("ImageButton", {
				AutoButtonColor = false,
				BackgroundColor3 = Theme.color.black,
				BackgroundTransparency = M.panelTransparency,
				AnchorPoint = Vector2.new(0.5, 0.5),
				Position = UDim2.new(0.5, 0, 0.5, M.panelDrop),
				Size = UDim2.fromOffset(width, height),
				ZIndex = 2,
			}, {
				UICorner = e("UICorner", { CornerRadius = UDim.new(0, M.panelCorner) }),
				HubBar = not state.leaving and e("Frame", {
					BackgroundTransparency = 1,
					Position = UDim2.fromOffset(M.padding, 0),
					Size = UDim2.new(1, -2 * M.padding, 0, top),
				}, {
					Tabs = e(Components.TabBar, {
						tabs = Pages.tabs,
						selected = state.page,
						onSelect = Menu.setPage,
						small = small,
					}),
				}),
				PageViewClipper = not state.leaving and e("Frame", {
					BackgroundTransparency = 1,
					ClipsDescendants = true,
					Position = UDim2.fromOffset(M.padding, top),
					Size = UDim2.new(1, -2 * M.padding, 0, pageHeight),
				}, {
					PageView = e("ScrollingFrame", {
						BackgroundTransparency = 1,
						BorderSizePixel = 0,
						AnchorPoint = Vector2.new(0.5, 0.5),
						Position = UDim2.new(0.5, 0, 0.5, 2),
						Size = UDim2.new(1, 0, 1, -M.pageInset),
						CanvasSize = UDim2.new(),
						AutomaticCanvasSize = Enum.AutomaticSize.Y,
						ScrollBarThickness = M.scrollBarThickness,
						ScrollingDirection = Enum.ScrollingDirection.Y,
						VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar,
					}, {
						Padding = e("UIPadding", {
							PaddingTop = UDim.new(0, M.pageTopPadding),
							PaddingLeft = UDim.new(0, M.pageLeftPadding),
							PaddingRight = UDim.new(0, M.pageRightPadding),
						}),
						RowListLayout = e("UIListLayout", {
							SortOrder = Enum.SortOrder.LayoutOrder,
							HorizontalAlignment = Enum.HorizontalAlignment.Center,
						}),
						Content = e(page, { small = small }),
					}),
				}),
				BottomButtonFrame = not state.leaving and e(BottomButtons, {
					height = bottom,
					top = top + pageHeight,
				}),
				LeaveConfirmation = state.leaving and e(LeaveConfirm, { small = small }),
			}),
		}),
	})
end
