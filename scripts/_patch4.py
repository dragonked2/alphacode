with open('src/alphacode_tui/tui/app/tests/remote_startup_input_03/part_01.rs', 'rb') as f:
    data = f.read()

old = (
    b'    app.restore_session(&session_id);\r\n'
    b'\r\n'
    b'    assert!(\r\n'
    b'        app.hidden_queued_system_messages\r\n'
    b'            .iter()\r\n'
    b'            .any(|message| message.contains("Continue exactly where you left off"))\r\n'
    b'    );\r\n'
    b'    assert!(app.pending_turn);\r\n'
    b'    assert!(matches!(app.status, ProcessingStatus::Sending));\r\n'
    b'\r\n'
    b'    let _ = std::fs::remove_file(crate::session::session_path(&session_id).unwrap());\r\n'
    b'}\r\n'
)

new = (
    b'    app.restore_session(&session_id);\r\n'
    b'\r\n'
    b'    assert!(\r\n'
    b'        app.hidden_queued_system_messages\r\n'
    b'            .iter()\r\n'
    b'            .any(|message| message.contains("Continue exactly where you left off"))\r\n'
    b'    );\r\n'
    b'    // The reload-continuation system reminder is *queued* but NOT\r\n'
    b'    // auto-dispatched: dispatching it eagerly would lock the input field\r\n'
    b'    // for the whole model round-trip (the "inputpreserved prevents\r\n'
    b'    // typing" bug). The user must press Enter to send it. The local event\r\n'
    b'    // loop must therefore see the composer as idle.\r\n'
    b'    assert!(!app.is_processing);\r\n'
    b'    assert!(!app.pending_turn);\r\n'
    b'    assert!(matches!(app.status, ProcessingStatus::Idle));\r\n'
    b'\r\n'
    b'    let _ = std::fs::remove_file(crate::session::session_path(&session_id).unwrap());\r\n'
    b'}\r\n'
)

assert data.count(old) == 1, f'old found {data.count(old)}'
data = data.replace(old, new, 1)

with open('src/alphacode_tui/tui/app/tests/remote_startup_input_03/part_01.rs', 'wb') as f:
    f.write(data)
print('OK')
