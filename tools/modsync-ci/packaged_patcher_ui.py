#!/usr/bin/env python3
"""Drive authenticated packaged TSLPatcher windows in their owned Wine display."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import tempfile
import time
import uuid

MAX_REQUEST = 16 * 1024 * 1024
DEADLINE_SECONDS = 300
PREFIX_STARTUP_SECONDS = 90


class PrefixNotReady(Exception):
    """The owned cold Wine prefix has not created its drive map yet."""


# This helper operates Windows controls. The packaged installer receives no
# additional command-line arguments and remains responsible for all patching.
WINDOW_HELPER = r'''
#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
#include <windows.h>
#include <commctrl.h>
#include <tlhelp32.h>
#include <shlobj.h>
#include <stdio.h>
#include <wchar.h>
static const wchar_t *expected;
static int count;
static int same_path(const wchar_t *left, const wchar_t *right) {
  wchar_t a[32768], b[32768];
  DWORD x = GetFullPathNameW(left, 32768, a, NULL);
  DWORD y = GetFullPathNameW(right, 32768, b, NULL);
  return x && x < 32768 && y && y < 32768 && !_wcsicmp(a, b);
}
static int owned(HWND h) {
  DWORD pid = 0;
  GetWindowThreadProcessId(h, &pid);
  HANDLE p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
  if (!p)
    return 0;
  wchar_t path[32768];
  DWORD size = 32768;
  int ok = QueryFullProcessImageNameW(p, 0, path, &size) &&
           same_path(path, expected);
  CloseHandle(p);
  return ok;
}
static void textout(const wchar_t *s) {
  char out[32768];
  int n = WideCharToMultiByte(CP_UTF8, 0, s, -1, out, sizeof(out), NULL, NULL);
  if (n <= 0)
    return;
  for (int i = 0; i < n - 1; i++)
    if (out[i] == '|' || out[i] == '\n' || out[i] == '\r')
      out[i] = ' ';
  fputs(out, stdout);
}
static void line(HWND h, HWND root, char type) {
  wchar_t title[2048], kind[128];
  GetWindowTextW(h, title, 2048);
  GetClassNameW(h, kind, 128);
  printf("%c|%llu|%llu|%d|%d|", type, (unsigned long long)(UINT_PTR)h,
         (unsigned long long)(UINT_PTR)root, IsWindowEnabled(h),
         GetDlgCtrlID(h));
  textout(kind);
  putchar('|');
  textout(title);
  putchar('\n');
}
static BOOL CALLBACK child(HWND h, LPARAM root) {
  if (++count > 512)
    return FALSE;
  if (IsWindowVisible(h))
    line(h, (HWND)root, 'C');
  return TRUE;
}
static BOOL CALLBACK top(HWND h, LPARAM unused) {
  (void)unused;
  if (IsWindowVisible(h) && owned(h)) {
    if (++count > 512)
      return FALSE;
    line(h, h, 'W');
    EnumChildWindows(h, child, (LPARAM)h);
  }
  return TRUE;
}
static int abort_owned_processes(void) {
  HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE)
    return 0;
  PROCESSENTRY32W entry = {0};
  entry.dwSize = sizeof(entry);
  int terminated = 0;
  if (Process32FirstW(snapshot, &entry))
    do {
      HANDLE process =
          OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                      FALSE, entry.th32ProcessID);
      if (!process)
        continue;
      wchar_t path[32768];
      DWORD size = 32768;
      if (QueryFullProcessImageNameW(process, 0, path, &size) &&
          same_path(path, expected) && TerminateProcess(process, 125))
        terminated++;
      CloseHandle(process);
    } while (Process32NextW(snapshot, &entry));
  CloseHandle(snapshot);
  return terminated;
}
static HWND tree;
static BOOL CALLBACK findtree(HWND h, LPARAM unused) {
  (void)unused;
  wchar_t kind[128];
  GetClassNameW(h, kind, 128);
  if (!_wcsicmp(kind, L"SysTreeView32"))
    tree = h;
  return TRUE;
}
static int tree_text(HANDLE process, HWND view, HTREEITEM selected,
                     wchar_t *text) {
  TVITEMW item = {0};
  SIZE_T written;
  item.mask = TVIF_TEXT;
  item.hItem = selected;
  item.cchTextMax = 2048;
  void *remote = VirtualAllocEx(process, NULL, sizeof(item) + 4096,
                                MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  if (!remote)
    return 0;
  item.pszText = (wchar_t *)((char *)remote + sizeof(item));
  int ok = WriteProcessMemory(process, remote, &item, sizeof(item), &written) &&
           SendMessageW(view, TVM_GETITEMW, 0, (LPARAM)remote) &&
           ReadProcessMemory(process, item.pszText, text, 4096, &written);
  VirtualFreeEx(process, remote, 0, MEM_RELEASE);
  text[2047] = 0;
  return ok;
}
static HTREEITEM find_drive(HANDLE process, HWND view, HTREEITEM node,
                            int depth, int *budget) {
  for (; node; node = (HTREEITEM)SendMessageW(view, TVM_GETNEXTITEM, TVGN_NEXT,
                                              (LPARAM)node)) {
    wchar_t label[2048];
    if (--*budget < 0 || !tree_text(process, view, node, label))
      return NULL;
    if (wcsstr(label, L"(Z:)"))
      return node;
    if (depth < 3 &&
        (!wcscmp(label, L"Desktop") || !wcscmp(label, L"My Computer") ||
         !wcscmp(label, L"Computer"))) {
      SendMessageW(view, TVM_EXPAND, TVE_EXPAND, (LPARAM)node);
      Sleep(50);
      HTREEITEM found =
          find_drive(process, view,
                     (HTREEITEM)SendMessageW(view, TVM_GETNEXTITEM, TVGN_CHILD,
                                             (LPARAM)node),
                     depth + 1, budget);
      if (found)
        return found;
    }
  }
  return NULL;
}
static int selected_path_matches(HANDLE process, HWND view,
                                 const wchar_t *expected_path) {
  wchar_t path[32768];
  wcsncpy(path, expected_path + 3, 32767);
  path[32767] = 0;
  wchar_t *parts[64], *context = NULL, *part = wcstok(path, L"\\", &context);
  int length = 0;
  while (part) {
    if (length == 64)
      return 0;
    parts[length++] = part;
    part = wcstok(NULL, L"\\", &context);
  }
  HTREEITEM selected =
      (HTREEITEM)SendMessageW(view, TVM_GETNEXTITEM, TVGN_CARET, 0);
  for (int index = length - 1; index >= 0; index--) {
    wchar_t label[2048];
    if (!selected || !tree_text(process, view, selected, label) ||
        _wcsicmp(label, parts[index]))
      return 0;
    selected = (HTREEITEM)SendMessageW(view, TVM_GETNEXTITEM, TVGN_PARENT,
                                       (LPARAM)selected);
  }
  wchar_t drive[2048];
  return selected && tree_text(process, view, selected, drive) &&
         wcsstr(drive, L"(Z:)");
}
int wmain(int argc, wchar_t **argv) {
  if (argc < 3)
    return 2;
  expected = argv[2];
  if (!wcscmp(argv[1], L"list")) {
    EnumWindows(top, 0);
    return count > 512 ? 3 : 0;
  }
  if (!wcscmp(argv[1], L"abort")) {
    printf("terminated|%d\n", abort_owned_processes());
    return 0;
  }
  if (argc < 4)
    return 2;
  HWND h = (HWND)(UINT_PTR)_wcstoui64(argv[3], NULL, 10);
  if (!IsWindow(h) || !owned(h) || !IsWindowVisible(h) || !IsWindowEnabled(h))
    return 4;
  if (!wcscmp(argv[1], L"click")) {
    PostMessageW(h, BM_CLICK, 0, 0);
    return 0;
  }
  if (!wcscmp(argv[1], L"combo") && argc == 5) {
    int index = _wtoi(argv[4]);
    LRESULT n = SendMessageW(h, CB_GETCOUNT, 0, 0);
    if (index < 0 || index >= n || n > 256)
      return 5;
    if (SendMessageW(h, CB_SETCURSEL, index, 0) == CB_ERR)
      return 6;
    PostMessageW(GetParent(h), WM_COMMAND,
                 MAKEWPARAM(GetDlgCtrlID(h), CBN_SELCHANGE), (LPARAM)h);
    printf("selection|%lld\n", (long long)SendMessageW(h, CB_GETCURSEL, 0, 0));
    return 0;
  }
  if (!wcscmp(argv[1], L"folder") && argc == 5) {
    DWORD pid = 0;
    GetWindowThreadProcessId(h, &pid);
    HANDLE process =
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_OPERATION |
                        PROCESS_VM_READ | PROCESS_VM_WRITE,
                    FALSE, pid);
    if (!process)
      return 7;
    BOOL self_wow = FALSE, target_wow = FALSE;
    if (!IsWow64Process(GetCurrentProcess(), &self_wow) ||
        !IsWow64Process(process, &target_wow) || self_wow != target_wow) {
      CloseHandle(process);
      return 16;
    }
    EnumChildWindows(h, findtree, 0);
    if (!tree)
      return 9;
    if (wcsncmp(argv[4], L"Z:\\", 3))
      return 10;
    SIZE_T bytes = (wcslen(argv[4]) + 1) * sizeof(wchar_t), written = 0;
    void *remote = VirtualAllocEx(process, NULL, bytes,
                                  MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (!remote ||
        !WriteProcessMemory(process, remote, argv[4], bytes, &written) ||
        written != bytes)
      return 17;
    SendMessageW(h, BFFM_SETSELECTIONW, TRUE, (LPARAM)remote);
    int direct = 0;
    for (int attempt = 0; attempt < 10 && !direct; attempt++) {
      Sleep(100);
      direct = selected_path_matches(process, tree, argv[4]);
    }
    VirtualFreeEx(process, remote, 0, MEM_RELEASE);
    if (direct) {
      CloseHandle(process);
      fputs("method|direct\nselectedpath|", stdout);
      textout(argv[4]);
      putchar('\n');
      return 0;
    }
    int budget = 512;
    HTREEITEM selected =
        find_drive(process, tree,
                   (HTREEITEM)SendMessageW(tree, TVM_GETNEXTITEM, TVGN_ROOT, 0),
                   0, &budget);
    if (!selected || wcsncmp(argv[4], L"Z:\\", 3))
      return 10;
    wchar_t path[32768];
    wcsncpy(path, argv[4] + 3, 32767);
    path[32767] = 0;
    wchar_t *context = NULL, *part = wcstok(path, L"\\", &context);
    while (part) {
      SendMessageW(tree, TVM_EXPAND, TVE_EXPAND, (LPARAM)selected);
      Sleep(100);
      HTREEITEM match = NULL,
                child = (HTREEITEM)SendMessageW(tree, TVM_GETNEXTITEM,
                                                TVGN_CHILD, (LPARAM)selected);
      for (; child; child = (HTREEITEM)SendMessageW(tree, TVM_GETNEXTITEM,
                                                    TVGN_NEXT, (LPARAM)child)) {
        wchar_t label[2048];
        if (--budget < 0 || !tree_text(process, tree, child, label))
          return 11;
        if (!_wcsicmp(label, part)) {
          if (match)
            return 12;
          match = child;
        }
      }
      if (!match)
        return 13;
      selected = match;
      part = wcstok(NULL, L"\\", &context);
    }
    SendMessageW(tree, TVM_SELECTITEM, TVGN_CARET, (LPARAM)selected);
    Sleep(100);
    if ((HTREEITEM)SendMessageW(tree, TVM_GETNEXTITEM, TVGN_CARET, 0) !=
        selected)
      return 14;
    if (!selected_path_matches(process, tree, argv[4]))
      return 18;
    CloseHandle(process);
    fputs("method|literal-tree\nselectedpath|", stdout);
    textout(argv[4]);
    putchar('\n');
    return 0;
  }
  return 2;
}
'''


def ordinary(path, directory=False):
    path = Path(path).absolute()
    for part in reversed((path, *path.parents)):
        if part.is_symlink():
            raise ValueError('packaged_ui_link_rejected')
    if not (path.is_dir() if directory else path.is_file()):
        raise ValueError('packaged_ui_path_kind')
    return path


def read_request(path, game):
    path = ordinary(path)
    if path.stat().st_size > MAX_REQUEST:
        raise ValueError('packaged_ui_request_bound')
    data = json.loads(path.read_text())
    identity = data.get('requestId')
    if (str(uuid.UUID(identity)) != path.stem or data.get('schemaVersion') != 1
            or data.get('backend') != 'mod-packaged'
            or data.get('status') != 'awaitingHeadlessInteraction'
            or Path(data.get('gameTarget', '')).absolute() != game):
        raise ValueError('packaged_ui_request_identity')
    if not re.fullmatch(r':[0-9]{1,5}(?:\.0)?', data.get('headlessDisplay', '')):
        raise ValueError('packaged_ui_display')
    namespace = data.get('namespaceIndex')
    if namespace is not None and (type(namespace) is not int or not 0 <= namespace < 256):
        raise ValueError('packaged_ui_namespace')
    executable = ordinary(data['stagedExecutable'])
    with executable.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    if digest != data.get('executableSha256'):
        raise ValueError('packaged_ui_executable_changed')
    prefix = Path(data['launch']['environmentOverrides']['WINEPREFIX']).absolute()
    if not prefix.is_relative_to(path.parent):
        raise ValueError('packaged_ui_prefix_scope')
    ordinary(prefix, directory=True)
    devices = prefix / "dosdevices"
    if not devices.exists() and not devices.is_symlink():
        raise PrefixNotReady()
    ordinary(devices, directory=True)
    mapping = devices / "z:"
    if not mapping.exists() and not mapping.is_symlink():
        raise PrefixNotReady()
    if not mapping.is_symlink() or mapping.resolve(strict=True) != Path("/"):
        raise ValueError("packaged_ui_drive_mapping")
    return data, executable, prefix


def windows_path(path):
    return 'Z:' + str(path).replace('/', chr(92))


def interact(request, game):
    game = ordinary(game, directory=True)
    data, executable, prefix = read_request(request, game)
    evidence = Path(tempfile.mkdtemp(prefix='packaged-ui-' + data['requestId'] + '-',
                                     dir=Path(request).parent))
    report = {'schema_version': 1, 'request_id': data['requestId'],
              'backend': 'mod-packaged', 'game_target': str(game),
              'namespace_index': data.get('namespaceIndex'), 'actions': [], 'ok': False}
    environment = {key: os.environ[key] for key in ('PATH', 'HOME', 'LANG', 'LC_ALL', 'TMPDIR')
                   if key in os.environ}
    environment.update(DISPLAY=data['headlessDisplay'], WINEPREFIX=str(prefix),
                       WINEDEBUG='-all', WINEDLLOVERRIDES='mscoree,mshtml=')
    for key in ('WAYLAND_DISPLAY', 'DBUS_SESSION_BUS_ADDRESS', 'SESSION_MANAGER', 'XAUTHORITY'):
        environment.pop(key, None)
    try:
        compiler = shutil.which('i686-w64-mingw32-gcc')
        wine = shutil.which('wine')
        if not compiler or not wine:
            raise ValueError('packaged_ui_wine_or_mingw_unavailable')
        source, helper = evidence / 'controls.c', evidence / 'controls.exe'
        source.write_text(WINDOW_HELPER)
        subprocess.run([compiler, '-O2', '-municode', '-o', str(helper), str(source), '-luser32', '-lcomctl32'],
                       check=True, capture_output=True, timeout=30)
        with helper.open('rb') as stream:
            report['control_helper_sha256'] = hashlib.file_digest(stream, 'sha256').hexdigest()
        report['control_source_sha256'] = hashlib.sha256(WINDOW_HELPER.encode()).hexdigest()
        from PIL import ImageGrab
        target = windows_path(executable)

        first_control = True

        def control(action, *arguments):
            nonlocal first_control
            timeout = 180 if action == 'folder' else 90 if first_control else 12
            first_control = False
            result = subprocess.run([wine, str(helper), action, target, *map(str, arguments)],
                                    env=environment, capture_output=True, timeout=timeout)
            if result.returncode:
                raise ValueError('packaged_ui_control_refused_' + str(result.returncode))
            return result.stdout.decode('utf-8').strip()

        def act(kind, handle, *arguments):
            started = time.monotonic()
            result = control(kind, handle, *arguments)
            report['actions'].append({'action': kind, 'handle': handle,
                                      'arguments': list(arguments), 'result': result,
                                      'seconds': round(time.monotonic() - started, 3)})
            return result

        deadline = time.monotonic() + DEADLINE_SECONDS
        namespace_done = data.get('namespaceIndex') is None
        namespace_submitted = False
        install_clicked = folder_selected = exit_clicked = False
        frames = 0
        last_snapshot = None
        while time.monotonic() < deadline:
            current = json.loads(Path(request).read_text())
            if current.get('requestId') != data['requestId']:
                raise ValueError('packaged_ui_request_replaced')
            if current.get('status') != 'awaitingHeadlessInteraction':
                report['engine_status'] = current.get('status')
                if not install_clicked or not folder_selected:
                    raise ValueError('packaged_ui_engine_finished_before_selection')
                process = current.get('processReceipt') or {}
                if (current.get('status') != 'processAndLogVerified'
                        or type(process.get('exitCode')) is not int or process['exitCode'] != 0
                        or process.get('cancelled') is not False
                        or process.get('processGroupQuiescent') is not True
                        or process.get('noChildStarted') is not False
                        or process.get('failure') is not None
                        or current.get('writerQuiescenceVerified') is not True):
                    raise ValueError('packaged_ui_engine_did_not_verify_installation')
                report['ok'] = True
                break
            if exit_clicked:
                # ModSync closes the owned display while checking process/log
                # completion. Await that authenticated outcome without using X.
                time.sleep(0.1)
                continue
            snapshot = control('list')
            if snapshot != last_snapshot:
                if frames >= 40:
                    raise ValueError('packaged_ui_frame_bound')
                ImageGrab.grab(xdisplay=environment['DISPLAY']).save(evidence / f'frame-{frames:02d}.png')
                (evidence / f'frame-{frames:02d}.txt').write_text(snapshot)
                frames += 1
                last_snapshot = snapshot
            controls = []
            for line in snapshot.splitlines():
                fields = line.split('|', 6)
                if len(fields) != 7:
                    raise ValueError('packaged_ui_control_shape')
                controls.append(dict(zip(('type', 'handle', 'root', 'enabled', 'id', 'class', 'text'), fields)))
            windows = [item for item in controls if item['type'] == 'W']
            if any(item['class'] not in ('TMainForm', 'TNamespaceForm', 'TApplication', '#32770', 'TMessageForm')
                   for item in windows):
                raise ValueError('packaged_ui_unsupported_window')
            dialogs = [item for item in windows if item['class'] in ('#32770', 'TMessageForm')]
            if len(dialogs) > 1:
                raise ValueError('packaged_ui_ambiguous_dialogs')
            if dialogs:
                dialog = dialogs[0]
                children = [item for item in controls if item['root'] == dialog['handle'] and item['type'] == 'C']
                text = ' '.join(item['text'] for item in children).lower()
                if dialog['class'] == 'TMessageForm':
                    result = subprocess.run(['tesseract', str(evidence / f'frame-{frames - 1:02d}.png'),
                                             'stdout', '--psm', '6'], capture_output=True, timeout=10)
                    if result.returncode or len(result.stdout) > 65536:
                        raise ValueError('packaged_ui_dialog_ocr_failed')
                    text += ' ' + result.stdout.decode('utf-8').lower()
                if dialog['text'].lower() == 'browse for folder' and 'folder' in text and install_clicked:
                    selected = act('folder', dialog['handle'], windows_path(game))
                    if selected.splitlines() not in ([
                            'method|direct', 'selectedpath|' + windows_path(game)], [
                            'method|literal-tree', 'selectedpath|' + windows_path(game)]):
                        raise ValueError('packaged_ui_folder_selection_differs')
                    buttons = [item for item in children if item['class'].lower() == 'button'
                               and item['text'].replace('&', '').lower() == 'ok' and item['enabled'] == '1']
                    if len(buttons) != 1:
                        raise ValueError('packaged_ui_folder_button_ambiguous')
                    folder_selected = True
                    act('click', buttons[0]['handle'])
                elif (dialog['text'] == 'Information' and folder_selected
                      and any(phrase in re.sub(r'[^a-z0-9]', '', text) for phrase in (
                          'theinstallerisfinishedpleasechecktheprogresslogfordetailsaboutwhathasbeendone',
                          'theinstalerisfinishedpleasechecktheprogresslogfordetailsaboutwhathasbeendone'))):
                    buttons = [item for item in children if item['class'].lower() in ('button', 'tbutton')
                               and item['text'].replace('&', '').lower() == 'ok' and item['enabled'] == '1']
                    if len(buttons) != 1:
                        raise ValueError('packaged_ui_completion_button_ambiguous')
                    act('click', buttons[0]['handle'])
                elif ('are you sure' in text or 'do you wish' in text or 'continue' in text) and install_clicked:
                    buttons = [item for item in children if item['class'].lower() == 'button'
                               and item['text'].replace('&', '').lower() == 'yes' and item['enabled'] == '1']
                    if len(buttons) != 1:
                        raise ValueError('packaged_ui_confirmation_ambiguous')
                    act('click', buttons[0]['handle'])
                else:
                    raise ValueError('packaged_ui_unrecognized_dialog')
            else:
                combos = [item for item in controls if 'combobox' in item['class'].lower() and item['enabled'] == '1']
                if not namespace_done:
                    if len(combos) != 1:
                        if frames > 2:
                            raise ValueError('packaged_ui_namespace_control_ambiguous')
                    else:
                        result = act('combo', combos[0]['handle'], data['namespaceIndex'])
                        if result != 'selection|' + str(data['namespaceIndex']):
                            raise ValueError('packaged_ui_namespace_selection_differs')
                        namespace_done = True
                namespace_windows = [item for item in windows if item['class'] == 'TNamespaceForm']
                if namespace_windows and not namespace_submitted:
                    if len(namespace_windows) != 1 or data.get('namespaceIndex') is None:
                        raise ValueError('packaged_ui_namespace_not_declared')
                    selectors = [item for item in controls if item['root'] == namespace_windows[0]['handle']
                                 and item['class'].lower() in ('tbutton', 'button')
                                 and item['text'].replace('&', '').strip().lower() == 'select'
                                 and item['enabled'] == '1']
                    if not namespace_done or len(selectors) != 1:
                        raise ValueError('packaged_ui_namespace_submit_ambiguous')
                    act('click', selectors[0]['handle'])
                    namespace_submitted = True
                buttons = [item for item in controls if item['class'].lower() in ('tbutton', 'tbitbtn', 'button')
                           and item['text'].replace('&', '').strip().lower().startswith('install mod')
                           and item['enabled'] == '1']
                if namespace_done and not install_clicked and len(buttons) == 1:
                    install_clicked = True
                    act('click', buttons[0]['handle'])
                if install_clicked and folder_selected:
                    exits = [item for item in controls if item['class'].lower() in ('tbutton', 'tbitbtn', 'button')
                             and item['text'].replace('&', '').lower() == 'exit' and item['enabled'] == '1']
                    if len(exits) == 1 and not buttons:
                        act('click', exits[0]['handle'])
                        exit_clicked = True
            time.sleep(0.5)
        else:
            raise ValueError('packaged_ui_interaction_deadline')
    except Exception as error:
        if 'helper' in locals() and helper.is_file() and 'target' in locals():
            try:
                aborted = subprocess.run([wine, str(helper), 'abort', target], env=environment,
                                         capture_output=True, timeout=20)
                report['owned_installer_abort_requested'] = aborted.returncode == 0
                report['owned_installer_abort_result'] = aborted.stdout.decode('utf-8', errors='replace').strip()
            except (OSError, subprocess.TimeoutExpired):
                report['owned_installer_abort_requested'] = False
        report['failure'] = str(error) if isinstance(error, ValueError) else type(error).__name__
    finally:
        (evidence / 'receipt.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


@contextmanager
def driver(requestdir, game):
    """Monitor only this operation's request directory and exact game target."""
    requests, game = ordinary(requestdir, directory=True), ordinary(game, directory=True)
    stopped, failures, children, seen = threading.Event(), [], {}, set()
    prefix_pending = {}

    def monitor():
        while not stopped.wait(0.25):
            try:
                for path in sorted(requests.glob('*.json')):
                    if path.name in seen:
                        continue
                    if path.stat().st_size > MAX_REQUEST:
                        raise ValueError('packaged_ui_request_bound')
                    data = json.loads(path.read_text())
                    if data.get('backend') != 'mod-packaged' or data.get('status') != 'awaitingHeadlessInteraction':
                        continue
                    try:
                        read_request(path, game)
                    except PrefixNotReady:
                        started = prefix_pending.setdefault(path.name, time.monotonic())
                        if time.monotonic() - started >= PREFIX_STARTUP_SECONDS:
                            raise ValueError('packaged_ui_prefix_startup_deadline') from None
                        continue
                    prefix_pending.pop(path.name, None)
                    seen.add(path.name)
                    log = (requests / ('packaged-ui-' + path.stem + '.log')).open('wb')
                    process = subprocess.Popen([sys.executable, __file__, '--request', str(path), '--game', str(game)],
                                               stdout=log, stderr=subprocess.STDOUT,
                                               env={key: os.environ[key] for key in (
                                                   'PATH', 'HOME', 'LANG', 'LC_ALL', 'TMPDIR')
                                                    if key in os.environ})
                    log.close()
                    children[path.name] = process
                for name, process in children.items():
                    if process.poll() not in (None, 0) and name not in failures:
                        failures.append(name)
            except Exception as error:
                failures.append(str(error) if isinstance(error, ValueError) else type(error).__name__)
                stopped.set()

    thread = threading.Thread(target=monitor, name='packaged-patcher-ui', daemon=True)
    thread.start()
    try:
        yield {'request_directory': str(requests), 'game_target': str(game), 'failures': failures}
    finally:
        stopped.set()
        thread.join(timeout=3)
        for name, process in children.items():
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
                failures.append('interaction_incomplete')
            if process.returncode != 0 and name not in failures:
                failures.append(name)
        if failures:
            raise RuntimeError('packaged_patcher_ui_failed ' + ','.join(failures[:8]))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--game', type=Path, required=True)
    arguments = parser.parse_args()
    raise SystemExit(0 if interact(arguments.request, arguments.game)['ok'] else 1)
