#Requires AutoHotkey v2.0
#SingleInstance Force
#Warn All, StdOut
; Harmonica Studio remote protocol: 1
; Harmonica Studio start offset: 1
; Generated event table. Empty template cannot play a song.
eventText := "
(
__EVENTS__
)"
events := []
for line in StrSplit(eventText, "`n", "`r") {
    if !InStr(line, ",")
        continue
    cols := StrSplit(line, ",")
    events.Push([Integer(cols[1]), cols[2], Integer(cols[3])])
}
; The same pure slicing path is exercised by validation, without timers or input.
if A_Args.Length && (A_Args[1] = "--validate" || A_Args[1] = "--dump-events") {
    if A_Args.Length >= 2
        events := EventsFrom(Integer(A_Args[2]))
    last := -1
    held := Map()
    for ev in events {
        if ev[1] < last
            ExitApp(2)
        last := ev[1]
        if ev[3] {
            if held.Has(ev[2])
                ExitApp(3)
            held[ev[2]] := true
        } else {
            if !held.Has(ev[2])
                ExitApp(4)
            held.Delete(ev[2])
        }
    }
    if held.Count
        ExitApp(5)
    if A_Args[1] = "--dump-events" {
        for ev in events
            FileAppend(ev[1] "," ev[2] "," ev[3] "`n", "*", "UTF-8")
    }
    ExitApp(0)
}
running := false
showCountdownTip := true
target := 0
nextEvent := 1
startAt := 0
activeEvents := events
defaultStartMs := A_Args.Length >= 4 ? Integer(A_Args[4]) : 0
playOffsetMs := 0
minimumPosition := 0.0
heldKeys := Map()
stopFile := A_Args.Length ? A_Args[1] : ""
commandFile := A_Args.Length >= 2 ? A_Args[2] : ""
statusFile := A_Args.Length >= 3 ? A_Args[3] : ""
lastCommandId := 0
state := "ready"
stateMessage := "演奏器已就绪；切到口琴界面按 F6 或从手机开始。"
position := 0.0
duration := events.Length ? events[events.Length][1] / 1000 : 0.0
OnExit(HandleExit)
if stopFile != "" || commandFile != ""
    SetTimer(CheckControl, 25)
if statusFile != ""
    SetTimer(PublishStatus, 100)
PublishStatus()
TrayTip("F6 开始或停止；F8 退出。切换窗口会自动停止。", "口琴演奏")

F6::TogglePlay()
F8::ExitApp()

CheckControl() {
    global stopFile, commandFile, lastCommandId, defaultStartMs
    ; Exit always takes precedence, including a request made during startup.
    if stopFile != "" && FileExist(stopFile)
        ExitApp()
    if commandFile = "" || !FileExist(commandFile)
        return
    try {
        ; Read one snapshot while allowing Rust's atomic file replacement.
        input := FileOpen(commandFile, 0x700, "UTF-8") ; read + share read/write/delete
        try {
            if input.Length > 4096
                return
            command := input.Read()
        } finally {
            input.Close()
        }
        parsed := ParseCommand(command)
        if !IsObject(parsed)
            return
        requestId := parsed["id"]
        if requestId <= lastCommandId
            return
        lastCommandId := requestId
        if parsed.Has("start_ms")
            defaultStartMs := parsed["start_ms"]
        if parsed["action"] = "stop"
            StopPlay("已从手机停止演奏。")
        else
            BeginPlay(false)
        PublishStatus()
    }
}

; Strict flat command schema, independent of JSON property order. Validate the
; entire object before consuming its id; reject unknown/duplicate fields.
ParseCommand(command) {
    if !RegExMatch(command, '^\s*\{([^{}]*)\}\s*$', &envelope)
        return false
    fields := Map()
    fields.CaseSense := "On"
    for member in StrSplit(envelope[1], ",") {
        if !RegExMatch(member, '^\s*"(id|action|start_ms)"\s*:\s*(.*?)\s*$', &pair)
            return false
        key := pair[1]
        value := pair[2]
        if fields.Has(key)
            return false
        if key = "action" {
            if !RegExMatch(value, '^"(play|stop)"$', &action)
                return false
            fields[key] := action[1]
        } else {
            ; Bounds keep conversion exact and consistent with the Rust writer.
            if !RegExMatch(value, '^(0|[1-9][0-9]*)$') || StrLen(value) > 16
                return false
            numericValue := Integer(value)
            if key = "id" && (numericValue < 1 || numericValue > 9007199254740991)
                return false
            if key = "start_ms" && numericValue > 86400000
                return false
            fields[key] := numericValue
        }
    }
    return fields.Has("id") && fields.Has("action") ? fields : false
}

JsonString(value) {
    value := StrReplace(value, '\', '\\')
    value := StrReplace(value, '"', '\"')
    value := StrReplace(value, "`r", '\r')
    value := StrReplace(value, "`n", '\n')
    value := StrReplace(value, "`t", '\t')
    return '"' value '"'
}

PublishStatus(*) {
    global statusFile, lastCommandId, state, stateMessage, position, duration, running, startAt
    global playOffsetMs, minimumPosition
    if statusFile = ""
        return
    current := running ? Min(duration, Max(minimumPosition, (NowMs() - startAt + playOffsetMs) / 1000)) : position
    data := '{"request_id":' lastCommandId ',"state":' JsonString(state)
         . ',"position":' Format('{:.3f}', current) ',"duration":' Format('{:.3f}', duration)
         . ',"message":' JsonString(stateMessage) '}'
    temporary := statusFile "." ProcessExist() ".tmp"
    try {
        output := FileOpen(temporary, "w", "UTF-8-RAW")
        output.Write(data)
        output.Close()
        ; Replace in the same directory; readers see a complete old/new JSON.
        if !DllCall("MoveFileExW", "Str", temporary, "Str", statusFile, "UInt", 9, "Int")
            FileDelete(temporary)
    }
}

HandleExit(*) {
    global state, stateMessage, running, position
    running := false
    ReleaseKeys()
    state := "idle"
    stateMessage := "演奏器已退出。"
    position := 0.0
    PublishStatus()
}

NowMs() {
    static frequency := 0
    if !frequency
        DllCall("QueryPerformanceFrequency", "Int64*", &frequency)
    counter := 0
    DllCall("QueryPerformanceCounter", "Int64*", &counter)
    return counter * 1000 / frequency
}

ReleaseKeys() {
    global heldKeys
    for key, _ in heldKeys {
        try SendEvent("{" key " up}")
    }
    heldKeys.Clear()
}

StopPlay(message := "已停止演奏。", finished := false) {
    global running, state, stateMessage, position, duration
    running := false
    SetTimer(Tick, 0)
    ReleaseKeys()
    ToolTip()
    state := "ready"
    stateMessage := message
    position := finished ? duration : 0.0
    PublishStatus()
}

TogglePlay() {
    global running
    if running
        StopPlay("已按 F6 停止演奏。")
    else
        BeginPlay()
}

EventsFrom(startMs) {
    global events
    if startMs = 0
        return events
    if startMs < 0 || !events.Length || startMs >= events[events.Length][1]
        return []
    heldAtStart := Map()
    remaining := []
    for tableEvent in events {
        if tableEvent[1] <= startMs {
            if tableEvent[3]
                heldAtStart[tableEvent[2]] := true
            else if heldAtStart.Has(tableEvent[2])
                heldAtStart.Delete(tableEvent[2])
        } else
            remaining.Push([tableEvent[1] - startMs + 100, tableEvent[2], tableEvent[3]])
    }
    clipped := []
    ; Restore modifiers 25 ms before the first held note; keep later times intact.
    for key, _ in heldAtStart {
        if InStr(key, "Button")
            clipped.Push([75, key, 1])
    }
    for key, _ in heldAtStart {
        if !InStr(key, "Button")
            clipped.Push([100, key, 1])
    }
    for tableEvent in remaining
        clipped.Push(tableEvent)
    return clipped
}

BeginPlay(showTip := true) {
    global running, target, nextEvent, startAt, events
    global state, stateMessage, position
    global activeEvents, defaultStartMs, playOffsetMs, minimumPosition
    global showCountdownTip
    ; A repeated phone request never toggles a playing song off.
    if running
        return
    if !events.Length {
        StopPlay("尚未加入曲谱，请先转换 MIDI。")
        return
    }
    activeEvents := EventsFrom(defaultStartMs)
    if !activeEvents.Length {
        StopPlay("心动片段起点已超出曲谱，请在电脑上重新设置。")
        return
    }
    target := WinExist("A")
    title := ""
    try title := WinGetTitle("ahk_id " target)
    if !RegExMatch(title, "i)三角洲|Delta.?Force|口琴钢琴测试台") {
        StopPlay("请先将电脑切换到三角洲口琴界面，再开始演奏。")
        return
    }
    nextEvent := 1
    showCountdownTip := showTip
    startAt := NowMs() + 3000
    playOffsetMs := defaultStartMs ? defaultStartMs - 100 : 0
    minimumPosition := defaultStartMs / 1000
    running := true
    position := minimumPosition
    state := "countdown"
    stateMessage := "3 秒后开始演奏；F6 停止，F8 退出。"
    SetKeyDelay(-1, -1)
    SetMouseDelay(-1)
    SetTimer(Tick, 5)
    PublishStatus()
}

Tick() {
    global running, target, nextEvent, startAt, activeEvents, heldKeys
    global state, stateMessage
    global showCountdownTip
    if !running
        return
    if !WinActive("ahk_id " target) {
        StopPlay("电脑已切换窗口，演奏自动停止。")
        return
    }
    elapsed := NowMs() - startAt
    if elapsed < 0 {
        state := "countdown"
        stateMessage := Ceil(-elapsed/1000) " 秒后开始演奏；F6 停止，F8 退出。"
        if showCountdownTip
            ToolTip("口琴将在 " Ceil(-elapsed/1000) " 秒后开始；F6 停止")
        return
    }
    state := "playing"
    stateMessage := "正在游戏内演奏；F6 停止，F8 退出。"
    ToolTip()
    try {
        while running && nextEvent <= activeEvents.Length && activeEvents[nextEvent][1] <= elapsed {
            if !WinActive("ahk_id " target) {
                StopPlay("电脑已切换窗口，演奏自动停止。")
                return
            }
            currentEvent := activeEvents[nextEvent]
            ; Avoid sending a burst of stale notes after a long system stall.
            if elapsed - currentEvent[1] > 150 {
                StopPlay("系统延迟过大，演奏已停止。")
                TrayTip("系统延迟过大，演奏已停止。按 F6 可重新开始。", "口琴演奏")
                return
            }
            Critical("On")
            if currentEvent[3] {
                heldKeys[currentEvent[2]] := true
                SendEvent("{" currentEvent[2] " down}")
            } else {
                SendEvent("{" currentEvent[2] " up}")
                if heldKeys.Has(currentEvent[2])
                    heldKeys.Delete(currentEvent[2])
            }
            nextEvent += 1
            Critical("Off")
        }
        if nextEvent > activeEvents.Length
            StopPlay("演奏完成。", true)
    } catch {
        Critical("Off")
        StopPlay("输入发送失败，演奏已停止。")
    }
}
