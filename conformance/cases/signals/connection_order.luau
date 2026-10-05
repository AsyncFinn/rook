-- Order in which handlers of one signal run, and Once.
local e = Instance.new("BindableEvent")
for i = 1, 3 do
    e.Event:Connect(function(x) log("handler", i, x) end)
end
e.Event:Once(function(x) log("once", x) end)
e:Fire("first")
log("after Fire")
task.wait()
e:Fire("second")
task.wait()
