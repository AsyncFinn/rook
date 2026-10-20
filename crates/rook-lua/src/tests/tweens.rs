use super::*;

#[test]
fn tweens() {
    expect(
        r#"
        local TweenService = game:GetService("TweenService")
        print(TweenService:GetValue(0.5, Enum.EasingStyle.Linear, Enum.EasingDirection.In))
        print(TweenService:GetValue(0.5, Enum.EasingStyle.Quad, Enum.EasingDirection.In))
        print(TweenService:GetValue(0.5, Enum.EasingStyle.Quad, Enum.EasingDirection.Out))
        local p = Instance.new("Part")
        p.Transparency = 0
        local tw = TweenService:Create(p, TweenInfo.new(1, Enum.EasingStyle.Linear), { Transparency = 1, Position = Vector3.new(10, 0, 0) })
        print(tw.ClassName, tw.Instance == p, tw.PlaybackState)
        tw.Completed:Connect(function(state) print("completed", state, p.Transparency, p.Position) end)
        tw:Play()
        print(tw.PlaybackState)
        task.wait(0.5)
        print("half", math.round(p.Transparency * 10) / 10)
        print(pcall(function() TweenService:Create(p, TweenInfo.new(), { Anchored = 5 }) end))
        task.wait(1)
        local f = Instance.new("Frame")
        print(f:TweenPosition(UDim2.new(1, 0, 0, 0), "Out", "Quad", 0.2, false, function(status) print("gui", status, f.Position) end))
        print(f:TweenPosition(UDim2.new(0, 5, 0, 0)))
        "#,
        120,
        &[
            "0.5",
            "0.25",
            "0.75",
            "Tween true Enum.PlaybackState.Begin",
            "Enum.PlaybackState.Playing",
            "half 0.5",
            "false TweenService:Create property named 'Anchored' cannot be tweened due to type mismatch (property is a 'boolean', but given type is 'number')",
            "completed Enum.PlaybackState.Completed 1 10, 0, 0",
            "true",
            "false",
            "gui Enum.TweenStatus.Completed {1, 0}, {0, 0}",
        ],
    );
}

#[test]
fn tweens_replay_after_finishing() {
    expect(
        r#"
        local TweenService = game:GetService("TweenService")
        local p = Instance.new("Part")
        p.Transparency = 0
        local tw = TweenService:Create(p, TweenInfo.new(0.2, Enum.EasingStyle.Linear), { Transparency = 1 })
        tw:Play()
        tw.Completed:Wait()
        p.Transparency = 0
        tw:Play()
        print(tw.PlaybackState)
        task.wait(0.1)
        print("replay", math.round(p.Transparency * 10) / 10)
        tw.Completed:Wait()
        print("done", p.Transparency)
        local n = 0
        tw.Completed:Connect(function()
            n += 1
            if n < 3 then p.Transparency = 0 tw:Play() end
        end)
        p.Transparency = 0
        tw:Play()
        task.wait(1)
        print("loops", n, tw.PlaybackState)
        local delayed = TweenService:Create(p, TweenInfo.new(0.1, Enum.EasingStyle.Linear, Enum.EasingDirection.In, 0, false, 0.1), { Transparency = 0 })
        delayed:Play()
        delayed:Pause()
        print(delayed.PlaybackState)
        delayed.Completed:Wait()
        print("delayed", p.Transparency)
        "#,
        180,
        &[
            "Enum.PlaybackState.Playing",
            "replay 0.5",
            "done 1",
            "loops 3 Enum.PlaybackState.Completed",
            "Enum.PlaybackState.Delayed",
            "delayed 0",
        ],
    );
}
