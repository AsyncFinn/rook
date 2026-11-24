-- Steps between minus and plus buttons: `value` (0 to `steps`, 10 by
-- default), `onChange(value)`. The minus hides at 0 and the plus at the
-- top, as Roblox's do. Pressing on the steps sets the value under the
-- cursor and follows it until the button comes up, wherever the cursor
-- goes. With `box` (Camera Sensitivity's), the slider ends short of the
-- row and a text box past it shows `box.text` and gives what's typed to
-- `box.onSubmit(text)`.

local CorePackages = game:GetService("CorePackages")
local UserInputService = game:GetService("UserInputService")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)
local Icon = require(script.Parent.Icon)

local e = React.createElement

type Box = { text: string, onSubmit: (string) -> () }

return function(props: { value: number, steps: number?, onChange: (number) -> (), box: Box? })
	local steps = props.steps or 10
	local stepsRef = React.useRef(nil)
	local dragging = React.useRef(false)
	local S = Theme.slider

	local function choose(at)
		at = math.clamp(at, 0, steps)
		if at ~= props.value then
			props.onChange(at)
		end
	end
	-- The drag's connections outlive a render; they call the latest one.
	local chooseRef = React.useRef(choose)
	chooseRef.current = choose

	local function follow(x)
		local frame = stepsRef.current
		if frame and frame.AbsoluteSize.X > 0 then
			chooseRef.current(math.ceil((x - frame.AbsolutePosition.X) / frame.AbsoluteSize.X * steps))
		end
	end

	React.useEffect(function()
		local moved = UserInputService.InputChanged:Connect(function(input)
			if dragging.current and input.UserInputType == Enum.UserInputType.MouseMovement then
				follow(input.Position.X)
			end
		end)
		local ended = UserInputService.InputEnded:Connect(function(input)
			if input.UserInputType == Enum.UserInputType.MouseButton1 then
				dragging.current = false
			end
		end)
		return function()
			moved:Disconnect()
			ended:Disconnect()
		end
	end, {})

	local blocks = {}
	for i = 1, steps do
		local color = if i <= props.value then Theme.color.stepOn else Theme.color.stepOff
		local children = nil
		-- The ends are round on the outside only: a square Filler covers
		-- the inner quarter.
		if i == 1 or i == steps then
			children = {
				Corner = e("UICorner", { CornerRadius = UDim.new(0, 8) }),
				Filler = e("Frame", {
					BorderSizePixel = 0,
					BackgroundColor3 = color,
					Position = UDim2.fromScale(if i == 1 then 0.75 else 0, 0),
					Size = UDim2.fromScale(0.25, 1),
				}),
			}
		end
		blocks["Step" .. i] = e("Frame", {
			BorderSizePixel = 0,
			BackgroundColor3 = color,
			AnchorPoint = Vector2.new(0, 0.5),
			Position = UDim2.new((i - 1) / steps, S.stepGap / 2, 0.5, 0),
			Size = UDim2.new(1 / steps, -S.stepGap, 0, S.stepHeight),
			ZIndex = 3,
		}, children)
	end
	blocks.Drag = e("TextButton", {
		Text = "",
		AutoButtonColor = false,
		BackgroundTransparency = 1,
		Size = UDim2.fromScale(1, 1),
		ZIndex = 4,
		[React.Event.InputBegan] = function(_, input)
			if input.UserInputType == Enum.UserInputType.MouseButton1 then
				dragging.current = true
				follow(input.Position.X)
			end
		end,
	})

	local function side(icon, direction, shown)
		return e("ImageButton", {
			AutoButtonColor = false,
			BackgroundTransparency = 1,
			Visible = shown,
			AnchorPoint = Vector2.new(if direction == 1 then 1 else 0, 0.5),
			Position = UDim2.fromScale(if direction == 1 then 1 else 0, 0.5),
			Size = UDim2.new(0, S.buttonWidth, 0, Theme.row.height),
			ZIndex = 3,
			[React.Event.Activated] = function()
				choose(props.value + direction)
			end,
		}, {
			Icon = e(Icon, { name = icon, size = S.iconSize, ImageColor3 = Theme.color.arrow }),
		})
	end

	local box = props.box
	local reserve = if box then S.boxWidth + 2 * S.boxGap else 0
	return e("Frame", {
		BackgroundTransparency = 1,
		AnchorPoint = Vector2.new(0, 0.5),
		Position = UDim2.fromScale(0, 0.5),
		Size = UDim2.new(1, -reserve, 0, if box then S.boxHeight else Theme.row.height),
	}, {
		StepsContainer = e("Frame", {
			BackgroundTransparency = 1,
			AnchorPoint = Vector2.new(0.5, 0.5),
			Position = UDim2.fromScale(0.5, 0.5),
			Size = UDim2.new(1, -100, 1, 0),
			ref = stepsRef,
		}, blocks),
		LeftButton = side("circle-minus", -1, props.value > 0),
		RightButton = side("circle-plus", 1, props.value < steps),
		CameraSensitivityTextBox = box and e("TextBox", {
			Text = box.text,
			PlaceholderText = box.text,
			ClearTextOnFocus = true,
			BorderSizePixel = 0,
			FontFace = Theme.font.regular,
			TextSize = S.boxTextSize,
			TextColor3 = Theme.color.white,
			BackgroundColor3 = Theme.color.field,
			AnchorPoint = Vector2.new(0, 0.5),
			Position = UDim2.new(1, S.boxGap, 0.5, 0),
			Size = UDim2.new(0, S.boxWidth, 0.8, 0),
			ZIndex = 3,
			[React.Event.FocusLost] = function(rbx)
				box.onSubmit(rbx.Text)
				rbx.Text = box.text
			end,
		}, {
			Corner = e("UICorner"),
		}),
	})
end
