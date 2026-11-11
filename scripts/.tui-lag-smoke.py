"""Temporary native-console editor smoke test. Never submits a provider prompt."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
EXE = Path('C:/Users/or0to/AppData/Local/Temp/alphacode-target/release/alphacode.exe')
k = c.WinDLL('kernel32', use_last_error=True)
u = c.WinDLL('user32', use_last_error=True)

class Coord(c.Structure):
    _fields_ = [('X', w.SHORT), ('Y', w.SHORT)]
class Rect(c.Structure):
    _fields_ = [('Left', w.SHORT), ('Top', w.SHORT), ('Right', w.SHORT), ('Bottom', w.SHORT)]
class Info(c.Structure):
    _fields_ = [('size', Coord), ('cursor', Coord), ('attributes', w.WORD), ('window', Rect), ('max_size', Coord)]
class Key(c.Structure):
    _fields_ = [('down', w.BOOL), ('repeat', w.WORD), ('vk', w.WORD), ('scan', w.WORD), ('char', w.WCHAR), ('modifiers', w.DWORD)]
class Event(c.Union):
    _fields_ = [('key', Key), ('padding', c.c_byte * 16)]
class Record(c.Structure):
    _fields_ = [('kind', w.WORD), ('event', Event)]
assert c.sizeof(Record) == 20

def api(name, args, result=w.BOOL):
    fn = getattr(k, name)
    fn.argtypes = args
    fn.restype = result
    return fn
create_file = api('CreateFileW', [w.LPCWSTR, w.DWORD, w.DWORD, c.c_void_p, w.DWORD, w.DWORD, w.HANDLE], w.HANDLE)
close_handle = api('CloseHandle', [w.HANDLE])
read_file = api('ReadFile', [w.HANDLE, c.c_void_p, w.DWORD, c.POINTER(w.DWORD), c.c_void_p])
write_file = api('WriteFile', [w.HANDLE, c.c_void_p, w.DWORD, c.POINTER(w.DWORD), c.c_void_p])
peek_pipe = api('PeekNamedPipe', [w.HANDLE, c.c_void_p, w.DWORD, c.POINTER(w.DWORD), c.POINTER(w.DWORD), c.POINTER(w.DWORD)])
attach_console = api('AttachConsole', [w.DWORD])
free_console = api('FreeConsole', [])
console_info = api('GetConsoleScreenBufferInfo', [w.HANDLE, c.POINTER(Info)])
read_console = api('ReadConsoleOutputCharacterW', [w.HANDLE, w.LPWSTR, w.DWORD, Coord, c.POINTER(w.DWORD)])
write_input = api('WriteConsoleInputW', [w.HANDLE, c.POINTER(Record), w.DWORD, c.POINTER(w.DWORD)])
set_window = api('SetConsoleWindowInfo', [w.HANDLE, w.BOOL, c.POINTER(Rect)])
set_buffer = api('SetConsoleScreenBufferSize', [w.HANDLE, Coord])
u.GetForegroundWindow.restype = w.HWND
u.SetForegroundWindow.argtypes = [w.HWND]
original_window = u.GetForegroundWindow()
INVALID = c.c_void_p(-1).value

def check(ok, label):
    if not ok:
        raise c.WinError(c.get_last_error(), label)

def open_handle(name):
    handle = create_file(name, 0xC0000000, 3, None, 3, 0, None)
    if handle == INVALID:
        raise c.WinError(c.get_last_error(), name)
    return handle

with tempfile.TemporaryDirectory(prefix='.tui-lag-smoke-', dir=ROOT / 'scripts') as temp:
    home = Path(temp) / 'home'
    runtime = Path(temp) / 'runtime'
    home.mkdir()
    runtime.mkdir()
    (home / 'config.toml').write_text('[display]\ndebug_socket = true\n[launch_hotkeys]\nenabled = false\n', encoding='utf-8')
    env = os.environ.copy()
    env.update(ALPHACODE_HOME=str(home), ALPHACODE_RUNTIME_DIR=str(runtime),
               ALPHACODE_SOCKET=str(runtime / 'smoke.sock'), ALPHACODE_DEBUG_CONTROL='1',
               ALPHACODE_TELEMETRY='0', TERM='xterm-256color', COLORTERM='truecolor')
    env.pop('NO_COLOR', None)
    for name in ['ALPHACODE_RESUMING', 'ALPHACODE_NAMED_PROVIDER_PROFILE', 'ALPHACODE_CLIENT_SELFDEV']:
        env.pop(name, None)
    socket = runtime / 'smoke-debug.sock'
    normalized = str(socket).replace('\\', '/').lower()
    pipe = '\\\\.\\pipe\\' + socket.stem + '-' + hashlib.sha256(normalized.encode()).hexdigest()[:16]
    common = [str(EXE), '--no-update', '--no-selfdev']
    server = client = None
    in_handle = out_handle = None
    request_id = 0
    log_path = Path(temp) / 'server.log'
    log = log_path.open('wb')
    result = {'binary': str(EXE), 'provider_prompts_sent': 0, 'checks': [], 'native_console_samples': []}

    def debug(command, timeout=5):
        nonlocal_dummy = None
        global request_id
        request_id += 1
        deadline = time.perf_counter() + timeout
        while True:
            handle = create_file(pipe, 0xC0000000, 0, None, 3, 0, None)
            if handle != INVALID:
                break
            if time.perf_counter() >= deadline:
                raise TimeoutError('debug pipe not ready: ' + command[:80])
            if server and server.poll() is not None:
                raise RuntimeError('temporary server exited')
            time.sleep(0.01)
        try:
            data = (json.dumps({'type': 'debug_command', 'id': request_id, 'command': command, 'session_id': None}, ensure_ascii=False) + '\n').encode()
            count = w.DWORD()
            offset = 0
            while offset < len(data):
                chunk = data[offset:offset + 65536]
                check(write_file(handle, c.create_string_buffer(chunk), len(chunk), c.byref(count), None), 'debug write')
                offset += count.value
            response = bytearray()
            available = w.DWORD()
            while b'\n' not in response:
                check(peek_pipe(handle, None, 0, None, c.byref(available), None), 'debug peek')
                if available.value:
                    buffer = c.create_string_buffer(min(available.value, 65536))
                    check(read_file(handle, buffer, len(buffer), c.byref(count), None), 'debug read')
                    response.extend(buffer.raw[:count.value])
                elif time.perf_counter() >= deadline:
                    raise TimeoutError('debug response: ' + command[:80])
                else:
                    time.sleep(0.001)
            value = json.loads(response.split(b'\n', 1)[0])
            if value.get('type') != 'debug_response' or not value.get('ok'):
                raise RuntimeError(str(value))
            return value['output']
        finally:
            close_handle(handle)

    def state():
        return json.loads(debug('client:state'))

    def wait_state(expected, cursor=None, timeout=3):
        deadline = time.perf_counter() + timeout
        while True:
            value = state()
            if value['input'] == expected and (cursor is None or value['cursor_pos'] == cursor):
                assert not value['processing'] and value['queued_messages'] == 0
                return value
            if time.perf_counter() >= deadline:
                raise AssertionError({'expected_bytes': len(expected.encode()), 'actual_bytes': len(value['input'].encode()), 'expected_cursor': cursor, 'actual_cursor': value['cursor_pos']})
            time.sleep(0.005)

    def screen():
        global out_handle
        close_handle(out_handle)
        out_handle = open_handle('CONOUT$')
        info = Info()
        check(console_info(out_handle, c.byref(info)), 'screen dimensions')
        rows = []
        width = info.window.Right - info.window.Left + 1
        for row in range(info.window.Top, info.window.Bottom + 1):
            buffer = c.create_unicode_buffer(width + 1)
            count = w.DWORD()
            check(read_console(out_handle, buffer, width, Coord(info.window.Left, row), c.byref(count)), 'read screen')
            rows.append(buffer[:count.value])
        return '\n'.join(rows), (width, len(rows))

    def wait_screen(marker, absent=None, timeout=4):
        deadline = time.perf_counter() + timeout
        while True:
            text, dimensions = screen()
            flattened = ''.join(ch for ch in text if not ch.isspace() and not ('\u2500' <= ch <= '\u257f'))
            if marker in flattened and (absent is None or absent not in flattened):
                return dimensions
            if time.perf_counter() >= deadline:
                print('SMOKE_INPUT_STATE ' + json.dumps(state(), ensure_ascii=False), flush=True)
                print('SMOKE_CURRENT_FRAME ' + debug('client:screen-json')[:3000], flush=True)
                raise AssertionError('console marker not painted: ' + marker + '\n' + text.encode('utf-8', 'backslashreplace').decode())
            time.sleep(0.0005)

    def key(vk, char='\0', modifiers=0):
        assert client and client.poll() is None, 'only inject into the owned smoke console'
        records = (Record * 2)()
        for index, down in enumerate([True, False]):
            records[index].kind = 1
            records[index].event.key = Key(down, 1, vk, 0, char, modifiers)
        count = w.DWORD()
        check(write_input(in_handle, records, 2, c.byref(count)), 'native key injection')
        assert count.value == 2

    def resize(width, height):
        check(set_window(out_handle, True, c.byref(Rect(0, 0, 0, 0))), 'shrink window for resize')
        check(set_buffer(out_handle, Coord(width, height)), 'resize buffer')
        check(set_window(out_handle, True, c.byref(Rect(0, 0, width - 1, height - 1))), 'resize window')

    try:
        print('ALPHACODE_PROGRESS ' + json.dumps({'message': 'Starting isolated temporary server and Windows console editor'}), flush=True)
        server = subprocess.Popen(common + ['--provider', 'ollama', '--model', 'editor-smoke', 'serve', '--temporary-server', '--owner-pid', str(os.getpid()), '--temp-idle-timeout-secs', '90', '--server-name', 'tui-lag-smoke'], env=env, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        debug('server:info', timeout=20)
        client = subprocess.Popen(common, env=env, cwd=ROOT, creationflags=subprocess.CREATE_NEW_CONSOLE)
        deadline = time.perf_counter() + 20
        while True:
            try:
                value = state()
                break
            except RuntimeError as error:
                if time.perf_counter() >= deadline or client.poll() is not None:
                    raise
                time.sleep(0.05)
        free_console()
        check(attach_console(client.pid), 'attach only to owned editor console')
        in_handle = open_handle('CONIN$')
        out_handle = open_handle('CONOUT$')
        resize(120, 40)
        for index in range(20):
            debug('client:inject:assistant:Message ' + str(index) + '\nA stable transcript line with enough text to exercise wrapping. ' * 8)
        for target_bytes in [128, 64 * 1024, 512 * 1024]:
            draft = 'A draft line to edit with unicode support. ' * (target_bytes // 42 + 1) + 'TUILAGTAIL'
            debug('client:set_input:' + draft)
            wait_state(draft, len(draft.encode()))
            key(ord('X'), 'x')
            wait_state(draft + 'x', len(draft.encode()) + 1)
            wait_screen('TUILAGTAILx')
            key(8, '\b')
            dimensions = wait_screen('TUILAGTAIL', 'TUILAGTAILx')
            wait_state(draft, len(draft.encode()))
            timings = []
            for iteration in range(20):
                started = time.perf_counter()
                if iteration % 2 == 0:
                    key(ord('X'), 'x')
                    wait_screen('TUILAGTAILx')
                else:
                    key(8, '\b')
                    wait_screen('TUILAGTAIL', 'TUILAGTAILx')
                if iteration >= 4:
                    timings.append((time.perf_counter() - started) * 1000)
            wait_state(draft, len(draft.encode()))
            result['native_console_samples'].append({'draft_bytes': len(draft.encode()), 'screen_size': dimensions, 'samples': len(timings), 'key_to_console_buffer_ms': {'p50': statistics.median(timings), 'max': max(timings)}})
            result['checks'].append('native insert/backspace restores ' + str(len(draft.encode())) + '-byte draft and caret')
            print('ALPHACODE_PROGRESS ' + json.dumps({'message': result['checks'][-1]}), flush=True)
        resize(100, 35)
        wait_screen('TUILAGTAIL', 'TUILAGTAILx')
        key(36)
        wait_state(draft, 0)
        key(35)
        wait_state(draft, len(draft.encode()))
        assert wait_screen('TUILAGTAIL') == (100, 35)
        result['checks'].append('large draft resize and Home/End preserve content, caret and tail paint')
        unicode_draft = 'Ae\u0301👩🏽\u200d💻🇺🇸'
        debug('client:set_input:' + unicode_draft)
        wait_state(unicode_draft, len(unicode_draft.encode()))
        for expected in ['Ae\u0301👩🏽\u200d💻', 'Ae\u0301', 'A']:
            key(8, '\b')
            wait_state(expected, len(expected.encode()))
        result['checks'].append('native backspace deletes flag, ZWJ emoji and combining cluster atomically')
        selected = 'ab👩\u200d💻'
        debug('client:set_input:' + selected)
        wait_state(selected, len(selected.encode()))
        key(37, modifiers=0x10)
        key(8, '\b')
        wait_state('ab', 2)
        key(ord('Z'), '\x1a', 0x8)
        wait_state(selected, len(selected.encode()))
        result['checks'].append('native Shift+Left selection, backspace and Ctrl+Z preserve Unicode undo')
        key(ord('U'), '\x15', 0x8)
        wait_state('', 0)
        key(ord('Z'), '\x1a', 0x8)
        wait_state(selected, len(selected.encode()))
        result['checks'].append('native clear draft and undo restore exact content')
        debug('client:set_input:@src/alphacode_tui/tui/ui_in')
        wait_state('@src/alphacode_tui/tui/ui_in')
        key(9, '\t')
        deadline = time.perf_counter() + 5
        while True:
            completion = state()['input']
            if 'ui_input.rs' in completion:
                break
            if time.perf_counter() >= deadline:
                raise AssertionError('path Tab completion did not resolve: ' + repr(completion))
            time.sleep(0.01)
        result['checks'].append('native Tab path completion resolves ui_input.rs')
        debug('client:set_input:EDITORSMOKEOK')
        wait_state('EDITORSMOKEOK', len('EDITORSMOKEOK'))
        key(35)
        wait_screen('EDITORSMOKEOK')
        capture = json.loads(debug('client:screen-json'))
        assert capture['terminal_size'] == [100, 35]
        result['final_frame_id'] = capture['frame_id']
        result['final_frame_anomalies'] = capture['anomalies']
        assert not capture['anomalies'], capture['anomalies']
        result['checks'].append('actual editor frame captured at final dimensions without render anomalies')
        debug('client:quit')
        assert client.wait(timeout=10) == 0
        result['checks'].append('isolated editor exits successfully')
        print('TUI_NATIVE_CONSOLE_SMOKE ' + json.dumps(result, ensure_ascii=False), flush=True)
    except Exception:
        log.flush()
        print('TUI_SMOKE_SERVER_LOG ' + log_path.read_text(encoding='utf-8', errors='replace')[-12000:], flush=True)
        raise
    finally:
        for handle in [in_handle, out_handle]:
            if handle is not None:
                close_handle(handle)
        free_console()
        if client and client.poll() is None:
            client.terminate()
            client.wait(timeout=10)
        try:
            if server and server.poll() is None:
                try:
                    subprocess.run(common + ['server', 'stop', '--force', '--json'], env=env, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
                    server.wait(timeout=10)
                except (subprocess.TimeoutExpired, OSError):
                    if server.poll() is None:
                        server.terminate()
                        server.wait(timeout=10)
        finally:
            log.close()
            if original_window:
                u.SetForegroundWindow(original_window)
