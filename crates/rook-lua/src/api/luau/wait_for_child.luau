local std = ...
local task, coroutine, warn, error, type = std.task, std.coroutine, std.warn, std.error, std.type
return function(self, name, timeOut)
    if type(name) ~= "string" then
        error("Argument 1 missing or nil", 2)
    end
    local found = self:FindFirstChild(name)
    if found then
        return found
    end
    local thread = coroutine.running()
    local done = false
    local conn, warnTask, timeoutTask
    local function finish(value)
        if done then return end
        done = true
        conn:Disconnect()
        if warnTask then task.cancel(warnTask) end
        if timeoutTask then task.cancel(timeoutTask) end
        task.spawn(thread, value)
    end
    conn = self.ChildAdded:Connect(function(child)
        if child.Name == name then
            finish(child)
        end
    end)
    if timeOut then
        timeoutTask = task.delay(timeOut, finish, nil)
    else
        warnTask = task.delay(5, function()
            warnTask = nil
            if not done then
                warn(("Infinite yield possible on '%s:WaitForChild(\"%s\")'"):format(self:GetFullName(), name))
            end
        end)
    end
    return coroutine.yield()
end
