use crate::tests::*;

/// Setting the time any of three ways reports `ClockTime` and `TimeOfDay`
/// both, and a change under a second neither (Studio 0.748, conformance
/// case `lighting/clock`).
#[test]
fn every_way_of_setting_the_time_reports_both_properties() {
    expect(
        r#"
        local L = game:GetService("Lighting")
        L.TimeOfDay = "14:00:00"
        task.wait()
        local seen = {}
        L.Changed:Connect(function(name) table.insert(seen, name) end)
        L:GetPropertyChangedSignal("ClockTime"):Connect(function() table.insert(seen, "signal") end)
        local function report()
            task.wait()
            print(table.concat(seen, ","))
            table.clear(seen)
        end
        L.ClockTime = 10
        report()
        L.TimeOfDay = "11:00:00"
        report()
        L:SetMinutesAfterMidnight(12 * 60)
        report()
        L.ClockTime = 12.00005
        report()
        "#,
        10,
        &[
            "ClockTime,signal,TimeOfDay",
            "ClockTime,signal,TimeOfDay",
            "ClockTime,signal,TimeOfDay",
            "",
        ],
    );
}

/// A `TimeOfDay` that doesn't read raises Studio's error and keeps the old
/// time; one that does reads back normalized.
#[test]
fn time_of_day_reads_back_normalized_or_is_refused() {
    expect(
        r#"
        local L = game:GetService("Lighting")
        L.TimeOfDay = "-1:2"
        print(L.TimeOfDay, L.ClockTime)
        print(pcall(function() L.TimeOfDay = "8h" end))
        print(pcall(function() L.TimeOfDay = "" end))
        print(L.TimeOfDay)
        "#,
        0,
        &[
            "22:58:00 22.96666717529297",
            "false bad lexical cast: source type value could not be interpreted as target",
            "false invalid string position",
            "22:58:00",
        ],
    );
}
