#include <vector>
#include <string>

int main()
{
    // build/bin/a.bin
    // this is comment for bpl test binary-preview-lsp/target/binary-preview-lsp.exe
    // binary-preview-lsp\\target\\debug\\binary-preview-lsp.exe

    std::vector<int> values;
    uint32_t value = 0x12345678;
    uint32_t hex_value = 0xFFD01234;
    double value_d = 1.934534;
    uint32_t v = 1234567;
    uint32_t b = 0b01010101010;

    std::string exe_path = "D:/Workspace/binary-preview/binary-preview-lsp/target/debug/binary-preview-lsp.exe";
    // std::string lib_path = "D:/Workspace/rfsw_repo_local/LGIT_WIFI_OI/Win32/Debug/DUT0/ssh.dll";
    // D:\Workspace\vscode\SCK-QTS\apps\lib\LitePoint\lib\TestManager.lib
    // D:\Workspace\vscode\SCK-QTS\apps\lib\LitePoint\lib\TestManager.dll
    // D:\Workspace\vscode\SCK-QTS\apps\lib\QMSL\QMSL_MSVC22R.dll
    // D:\Workspace\vscode\SCK-QTS\apps\lib\QMSL\lib\QMSL_MSVC22R.lib
    // C:\Users\yeongho.jeon\Downloads\libqcocoa.dylib
    // C:\Users\yeongho.jeon\Downloads\ccmake

    return 0;
}
