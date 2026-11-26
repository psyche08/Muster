// SPDX-License-Identifier: Apache-2.0
// Session-zero supervisor. Task Scheduler owns the ordinary interactive logon.
// No network endpoint, capture, configuration input, or user token is held here.
#include <windows.h>
#include <taskschd.h>
#include <wrl/client.h>
#include <sddl.h>
#include <cwchar>

using Microsoft::WRL::ComPtr;

namespace {
// mingw-w64 taskschd.h omits the TASK_RUN_FLAGS enum.
constexpr LONG use_session_id = 0x4; // TASK_RUN_USE_SESSION_ID in the Windows SDK
constexpr LONG use_user_sid = 0x8; // TASK_RUN_USER_SID
constexpr wchar_t service_name[] = L"MusterStream";
constexpr wchar_t task_name[] = L"Muster Stream Worker";
HANDLE events[2]{};
SERVICE_STATUS_HANDLE status_handle;
SRWLOCK status_lock = SRWLOCK_INIT;
DWORD current_state = SERVICE_START_PENDING;
BSTR enrolled_sid;

void status(DWORD state, DWORD error = NO_ERROR) {
    AcquireSRWLockExclusive(&status_lock);
    // A stop delivered between registration and initialization wins.
    if (current_state != SERVICE_STOP_PENDING || state == SERVICE_STOPPED) {
        current_state = state;
        SERVICE_STATUS s{};
        s.dwServiceType = SERVICE_WIN32_OWN_PROCESS;
        s.dwCurrentState = state;
        s.dwWin32ExitCode = error;
        if (state == SERVICE_RUNNING)
            s.dwControlsAccepted = SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN | SERVICE_ACCEPT_SESSIONCHANGE;
        SetServiceStatus(status_handle, &s);
    }
    ReleaseSRWLockExclusive(&status_lock);
}

DWORD WINAPI control(DWORD code, DWORD, void*, void*) {
    switch (code) {
    case SERVICE_CONTROL_STOP:
    case SERVICE_CONTROL_SHUTDOWN:
        status(SERVICE_STOP_PENDING);
        SetEvent(events[0]);
        return NO_ERROR;
    case SERVICE_CONTROL_SESSIONCHANGE:
        SetEvent(events[1]);
        return NO_ERROR;
    case SERVICE_CONTROL_INTERROGATE:
        return NO_ERROR;
    default:
        return ERROR_CALL_NOT_IMPLEMENTED;
    }
}

HRESULT worker_task(ComPtr<IRegisteredTask>& task) {
    ComPtr<ITaskService> service;
    HRESULT hr = CoCreateInstance(CLSID_TaskScheduler, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&service));
    VARIANT empty{};
    if (FAILED(hr) || FAILED(hr = service->Connect(empty, empty, empty, empty))) return hr;
    ComPtr<ITaskFolder> root;
    BSTR root_path = SysAllocString(L"\\");
    if (!root_path) return E_OUTOFMEMORY;
    hr = service->GetFolder(root_path, &root);
    SysFreeString(root_path);
    if (FAILED(hr)) return hr;
    BSTR name = SysAllocString(task_name);
    if (!name) return E_OUTOFMEMORY;
    hr = root->GetTask(name, &task);
    SysFreeString(name);
    return hr;
}

void supervise() {
    ComPtr<IRunningTask> worker;
    DWORD selected = 0xffffffff;
    for (;;) {
        auto id = WTSGetActiveConsoleSessionId();
        if (worker) {
            TASK_STATE state{};
            if (id != selected) {
                worker->Stop();
                worker.Reset();
            } else if (FAILED(worker->Refresh()) || FAILED(worker->get_State(&state)) ||
                       (state != TASK_STATE_RUNNING && state != TASK_STATE_QUEUED)) {
                worker.Reset();
            }
        }
        if (!worker && id != 0 && id != 0xffffffff) {
            ComPtr<IRegisteredTask> task;
            if (SUCCEEDED(worker_task(task)) && WTSGetActiveConsoleSessionId() == id) {
                VARIANT empty{};
                // The fixed task has an interactive-token, least-privilege principal.
                if (SUCCEEDED(task->RunEx(empty, use_session_id | use_user_sid, id, enrolled_sid, &worker)))
                    selected = id;
            }
        }
        auto result = WaitForMultipleObjects(2, events, FALSE, 30000);
        if (result != WAIT_OBJECT_0 + 1 && result != WAIT_TIMEOUT) break;
    }
    if (worker) worker->Stop();
}

void WINAPI run(DWORD, LPWSTR*) {
    status_handle = RegisterServiceCtrlHandlerExW(service_name, control, nullptr);
    if (!status_handle) return;
    HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (FAILED(hr)) { status(SERVICE_STOPPED, ERROR_DLL_INIT_FAILED); return; }
    status(SERVICE_RUNNING);
    supervise();
    CoUninitialize();
    status(SERVICE_STOPPED);
}
} // namespace

int wmain(int argc, wchar_t** argv) {
    if (argc != 3 || wcscmp(argv[1], L"--service") != 0) return ERROR_BAD_ARGUMENTS;
    PSID sid = nullptr;
    if (!ConvertStringSidToSidW(argv[2], &sid)) return ERROR_INVALID_SID;
    LocalFree(sid);
    enrolled_sid = SysAllocString(argv[2]);
    if (!enrolled_sid) return ERROR_OUTOFMEMORY;
    events[0] = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    events[1] = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!events[0] || !events[1]) return ERROR_NOT_ENOUGH_MEMORY;
    SERVICE_TABLE_ENTRYW table[] = {{const_cast<LPWSTR>(service_name), run}, {nullptr, nullptr}};
    DWORD result = StartServiceCtrlDispatcherW(table) ? NO_ERROR : GetLastError();
    CloseHandle(events[1]);
    CloseHandle(events[0]);
    SysFreeString(enrolled_sid);
    return result;
}
