// NTDC's isolated Windows helper for LunaHook text capture.
// Process injection stays in this small child process, never in the Tauri app.
#include <windows.h>

#include <algorithm>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>
#include <vector>

namespace {

#ifdef _WIN64
constexpr wchar_t kLunaArch[] = L"64";
#else
constexpr wchar_t kLunaArch[] = L"32";
#endif

struct LunaThreadParam {
  uint32_t processId;
  // LunaHost's by-value callback ABI keeps the 64-bit fields 8-byte aligned
  // in both x86 and x64 builds. Make the padding explicit for MinGW x86.
  uint32_t alignment;
  uint64_t addr;
  uint64_t ctx;
  uint64_t ctx2;
};
static_assert(sizeof(LunaThreadParam) == 32, "Unexpected LunaHost callback ABI");
static_assert(offsetof(LunaThreadParam, addr) == 8, "Unexpected LunaHost callback ABI");

using LunaProcessEvent = void (*)(DWORD);
using LunaThreadEventMaybeEmbed = void (*)(const wchar_t*, const char*, LunaThreadParam, bool);
using LunaThreadEvent = void (*)(const wchar_t*, const char*, LunaThreadParam);
using LunaOutputCallback = void (*)(const wchar_t*, const char*, LunaThreadParam, const wchar_t*);
using LunaHostInfoHandler = void (*)(int, const wchar_t*);
using LunaHookInsertHandler = void (*)(DWORD, uint64_t, const wchar_t*);
using LunaEmbedCallback = void (*)(const wchar_t*, LunaThreadParam);
using LunaI18nQueryCallback = wchar_t* (*)(const wchar_t*);
using LunaEmuGameInfoCallback = void (*)(const wchar_t*, const wchar_t*, const wchar_t*);
using LunaStart = void (*)(LunaProcessEvent, LunaProcessEvent, LunaThreadEventMaybeEmbed,
                           LunaThreadEvent, LunaOutputCallback, LunaHostInfoHandler,
                           LunaHookInsertHandler, LunaEmbedCallback,
                           LunaI18nQueryCallback, LunaEmuGameInfoCallback);
using LunaConnectProcess = void (*)(DWORD);
using LunaCheckIfNeedInject = bool (*)(DWORD);
using LunaDetachProcess = void (*)(DWORD);
using LunaSettings = void (*)(int, bool, int, int, int, bool);
using LunaInsertHookCode = bool (*)(DWORD, const wchar_t*);

SRWLOCK g_output_lock = SRWLOCK_INIT;
DWORD g_target_pid = 0;
HMODULE g_luna_host = nullptr;
LunaDetachProcess g_detach = nullptr;
std::atomic<bool> g_connected{false};

std::string ToUtf8(const wchar_t* text, int length) {
  if (text == nullptr || length <= 0) return {};
  const int needed = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, length,
                                        nullptr, 0, nullptr, nullptr);
  if (needed <= 0 || needed > 256 * 1024) return {};
  std::string result(static_cast<size_t>(needed), '\0');
  const int written = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, length,
                                          result.data(), needed, nullptr, nullptr);
  if (written != needed) return {};
  return result;
}

std::string Base64(const std::string& bytes) {
  constexpr char alphabet[] =
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  std::string encoded;
  encoded.reserve((bytes.size() + 2) / 3 * 4);
  for (size_t i = 0; i < bytes.size(); i += 3) {
    const uint32_t a = static_cast<uint8_t>(bytes[i]);
    const uint32_t b = i + 1 < bytes.size() ? static_cast<uint8_t>(bytes[i + 1]) : 0;
    const uint32_t c = i + 2 < bytes.size() ? static_cast<uint8_t>(bytes[i + 2]) : 0;
    const uint32_t triple = (a << 16) | (b << 8) | c;
    encoded.push_back(alphabet[(triple >> 18) & 63]);
    encoded.push_back(alphabet[(triple >> 12) & 63]);
    if (i + 1 < bytes.size()) encoded.push_back(alphabet[(triple >> 6) & 63]);
    if (i + 2 < bytes.size()) encoded.push_back(alphabet[triple & 63]);
  }
  return encoded;
}

void WriteRecord(const char* prefix, const std::string& value) {
  HANDLE output = GetStdHandle(STD_OUTPUT_HANDLE);
  if (output == nullptr || output == INVALID_HANDLE_VALUE) return;
  std::string record(prefix);
  record += value;
  record += "\n";
  AcquireSRWLockExclusive(&g_output_lock);
  size_t offset = 0;
  while (offset < record.size()) {
    DWORD written = 0;
    const DWORD chunk = static_cast<DWORD>(std::min<size_t>(record.size() - offset, MAXDWORD));
    if (!WriteFile(output, record.data() + offset, chunk, &written, nullptr) || written == 0) break;
    offset += written;
  }
  ReleaseSRWLockExclusive(&g_output_lock);
}

void EmitError(const std::wstring& message) {
  WriteRecord("NTDC_ERROR:", Base64(ToUtf8(message.c_str(), static_cast<int>(message.size()))));
}

void OnProcessConnected(DWORD pid) {
  if (pid == g_target_pid) {
    g_connected.store(true);
    WriteRecord("NTDC_STATUS:", "attached");
  }
}
void OnProcessDisconnected(DWORD) {}
std::string HookSelectionKey(const wchar_t* hookcode, const char* name, LunaThreadParam thread) {
  const std::string code = ToUtf8(hookcode, hookcode == nullptr ? 0 : static_cast<int>(wcslen(hookcode)));
  // Keep split contexts separate. TYPEMOON also separates dialogue by call context.
  const bool exact_context = name != nullptr && std::string(name) == "typemoon";
  return code + '\x1f' + std::to_string(thread.ctx2) + '\x1f' +
      std::to_string(exact_context ? thread.ctx : 0);
}

void EmitHook(const wchar_t* hookcode, const char* name, LunaThreadParam thread, const wchar_t* sample = nullptr) {
  if (hookcode == nullptr || hookcode[0] == L'\0') return;
  const std::string code = ToUtf8(hookcode, static_cast<int>(wcslen(hookcode)));
  const std::string utf8_name = name == nullptr ? std::string() : std::string(name);
  const std::string utf8_sample = ToUtf8(sample, sample == nullptr ? 0 : static_cast<int>(wcslen(sample)));
  WriteRecord("NTDC_SOURCE:", Base64(HookSelectionKey(hookcode, name, thread)) + "|" +
      Base64(code) + "|" + Base64(utf8_name) + "|" + Base64(utf8_sample));
}

void OnThreadCreate(const wchar_t* hookcode, const char* name, LunaThreadParam thread, bool) {
  if (thread.processId == g_target_pid) EmitHook(hookcode, name, thread);
}
void OnThreadRemove(const wchar_t*, const char*, LunaThreadParam) {}
void OnHostInfo(int, const wchar_t*) {}
void OnHookInsert(DWORD, uint64_t, const wchar_t*) {}
void OnEmbed(const wchar_t*, LunaThreadParam) {}

void OnText(const wchar_t* hookcode, const char* name, LunaThreadParam thread, const wchar_t* text) {
  if (thread.processId != g_target_pid || text == nullptr) return;
  size_t length = 0;
  constexpr size_t kMaxTextCharacters = 32768;
  while (length < kMaxTextCharacters && text[length] != L'\0') ++length;
  if (length == 0 || length == kMaxTextCharacters) return;
  const std::string utf8 = ToUtf8(text, static_cast<int>(length));
  if (!utf8.empty() && hookcode != nullptr && hookcode[0] != L'\0') {
    EmitHook(hookcode, name, thread, text);
    WriteRecord("NTDC_TEXT:", Base64(HookSelectionKey(hookcode, name, thread)) + "|" + Base64(utf8));
  }
}

std::wstring HelperDirectory() {
  std::vector<wchar_t> path(32768, L'\0');
  const DWORD length = GetModuleFileNameW(nullptr, path.data(), static_cast<DWORD>(path.size()));
  if (length == 0 || length >= path.size()) return {};
  std::wstring result(path.data(), length);
  const size_t slash = result.find_last_of(L"\\/");
  if (slash == std::wstring::npos) return {};
  result.resize(slash + 1);
  return result;
}

bool InjectLunaHook(HANDLE process, const std::wstring& dll_path, std::wstring* error) {
  const size_t bytes = (dll_path.size() + 1) * sizeof(wchar_t);
  void* remote_path = VirtualAllocEx(process, nullptr, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  if (remote_path == nullptr) {
    *error = L"Could not allocate memory in the game process (" + std::to_wstring(GetLastError()) + L")";
    return false;
  }
  SIZE_T written = 0;
  if (!WriteProcessMemory(process, remote_path, dll_path.c_str(), bytes, &written) || written != bytes) {
    *error = L"Could not pass the hook DLL path to the game process (" + std::to_wstring(GetLastError()) + L")";
    VirtualFreeEx(process, remote_path, 0, MEM_RELEASE);
    return false;
  }

  HMODULE kernel = GetModuleHandleW(L"kernel32.dll");
  auto load_library = kernel == nullptr ? nullptr : GetProcAddress(kernel, "LoadLibraryW");
  if (load_library == nullptr) {
    *error = L"Windows could not resolve LoadLibraryW";
    VirtualFreeEx(process, remote_path, 0, MEM_RELEASE);
    return false;
  }
  HANDLE thread = CreateRemoteThread(process, nullptr, 0,
      reinterpret_cast<LPTHREAD_START_ROUTINE>(load_library), remote_path, 0, nullptr);
  if (thread == nullptr) {
    *error = L"Windows could not start the hook loader in the game (" + std::to_wstring(GetLastError()) + L")";
    VirtualFreeEx(process, remote_path, 0, MEM_RELEASE);
    return false;
  }
  const DWORD wait = WaitForSingleObject(thread, 10000);
  DWORD loaded = 0;
  const bool got_result = wait == WAIT_OBJECT_0 && GetExitCodeThread(thread, &loaded);
  CloseHandle(thread);
  VirtualFreeEx(process, remote_path, 0, MEM_RELEASE);
  if (!got_result || loaded == 0) {
    *error = wait == WAIT_TIMEOUT
        ? L"The game did not finish loading the hook DLL within 10 seconds"
        : L"The game rejected the hook DLL (" + std::to_wstring(GetLastError()) + L")";
    return false;
  }
  return true;
}

bool StopRequested() {
  HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
  if (input == nullptr || input == INVALID_HANDLE_VALUE) return false;
  DWORD available = 0;
  if (!PeekNamedPipe(input, nullptr, 0, nullptr, &available, nullptr)) {
    return GetLastError() == ERROR_BROKEN_PIPE;
  }
  if (available == 0) return false;
  char buffer[64] = {};
  DWORD read = 0;
  if (!ReadFile(input, buffer, static_cast<DWORD>(sizeof(buffer)), &read, nullptr)) return false;
  return std::string(buffer, read).find("stop") != std::string::npos;
}

void Detach() {
  if (g_detach != nullptr && g_target_pid != 0) g_detach(g_target_pid);
  g_detach = nullptr;
  // LunaHost has detached worker threads. It is safe to let the OS reclaim it
  // when this isolated helper exits, but not to unload it while those run.
}

int Fail(const std::wstring& message) {
  EmitError(message);
  Detach();
  return 1;
}

std::wstring DecodeHookArgument(const std::string& encoded) {
  const std::string alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  std::string bytes;
  uint32_t bits = 0;
  int count = 0;
  for (char letter : encoded) {
    const auto value = alphabet.find(letter);
    if (value == std::string::npos) return {};
    bits = (bits << 6) | static_cast<uint32_t>(value);
    count += 6;
    if (count >= 8) {
      count -= 8;
      bytes.push_back(static_cast<char>((bits >> count) & 255));
    }
  }
  const int length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, bytes.data(),
                                        static_cast<int>(bytes.size()), nullptr, 0);
  if (length <= 0) return {};
  std::wstring code(static_cast<size_t>(length), L'\0');
  MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, bytes.data(), static_cast<int>(bytes.size()), code.data(), length);
  return code;
}

}  // namespace

int main(int argc, char** argv) {
  DWORD pid = 0;
  std::wstring saved_hook;
  for (int i = 1; i + 1 < argc; ++i) {
    if (std::string(argv[i]) == "--pid") {
      char* end = nullptr;
      const unsigned long parsed = strtoul(argv[i + 1], &end, 10);
      if (end != argv[i + 1] && *end == '\0' && parsed > 0 && parsed <= MAXDWORD) {
        pid = static_cast<DWORD>(parsed);
      }
    } else if (std::string(argv[i]) == "--hook") {
      saved_hook = DecodeHookArgument(argv[i + 1]);
      if (saved_hook.empty()) {
        EmitError(L"saved-hook-invalid");
        return 2;
      }
    }
  }
  if (pid == 0) {
    EmitError(L"A valid --pid is required");
    return 2;
  }
  g_target_pid = pid;
  WriteRecord("NTDC_STATUS:", "starting");

  HANDLE process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_CREATE_THREAD |
                                   PROCESS_VM_OPERATION | PROCESS_VM_WRITE | SYNCHRONIZE,
                               FALSE, pid);
  if (process == nullptr) {
    return Fail(L"Could not open the game process (" + std::to_wstring(GetLastError()) +
                L"). Check that the game and companion run at the same privilege level.");
  }

  const std::wstring directory = HelperDirectory();
  const std::wstring host_path = directory + L"LunaHost" + kLunaArch + L".dll";
  const std::wstring hook_path = directory + L"LunaHook" + kLunaArch + L".dll";
  g_luna_host = LoadLibraryW(host_path.c_str());
  if (g_luna_host == nullptr) {
    CloseHandle(process);
    return Fail(L"LunaHook host files are missing or could not be loaded");
  }

  const auto start = reinterpret_cast<LunaStart>(GetProcAddress(g_luna_host, "Luna_Start"));
  const auto connect = reinterpret_cast<LunaConnectProcess>(GetProcAddress(g_luna_host, "Luna_ConnectProcess"));
  const auto needs_inject = reinterpret_cast<LunaCheckIfNeedInject>(GetProcAddress(g_luna_host, "Luna_CheckIfNeedInject"));
  g_detach = reinterpret_cast<LunaDetachProcess>(GetProcAddress(g_luna_host, "Luna_DetachProcess"));
  const auto settings = reinterpret_cast<LunaSettings>(GetProcAddress(g_luna_host, "Luna_Settings"));
  const auto insert_hook = reinterpret_cast<LunaInsertHookCode>(GetProcAddress(g_luna_host, "Luna_InsertHookCode"));
  if (start == nullptr || connect == nullptr || needs_inject == nullptr || g_detach == nullptr) {
    CloseHandle(process);
    return Fail(L"The installed LunaHook host does not expose the required API");
  }

  if (settings != nullptr) settings(200, true, 932, 8192, 1000, false);
  start(&OnProcessConnected, &OnProcessDisconnected, &OnThreadCreate, &OnThreadRemove,
        &OnText, &OnHostInfo, &OnHookInsert, &OnEmbed, nullptr, nullptr);
  connect(pid);

  if (needs_inject(pid)) {
    if (GetFileAttributesW(hook_path.c_str()) == INVALID_FILE_ATTRIBUTES) {
      CloseHandle(process);
      return Fail(L"The architecture-matched LunaHook DLL is missing");
    }
    std::wstring error;
    if (!InjectLunaHook(process, hook_path, &error)) {
      CloseHandle(process);
      return Fail(error);
    }
  }

  bool hook_restored = saved_hook.empty();
  while (WaitForSingleObject(process, 400) == WAIT_TIMEOUT) {
    if (StopRequested()) break;
    if (!hook_restored && g_connected.load()) {
      hook_restored = true;
      if (insert_hook == nullptr || !insert_hook(pid, saved_hook.c_str())) {
        EmitError(L"saved-hook-restore-failed");
      }
    }
  }
  Detach();
  CloseHandle(process);
  WriteRecord("NTDC_STATUS:", "stopped");
  return 0;
}
