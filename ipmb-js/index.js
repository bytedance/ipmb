const { platform, arch } = process;

switch (platform) {
    case 'darwin':
    switch (arch) {
        case 'arm64':
        module.exports = require('./aarch64-apple-darwin/release/ipmb_js.node');
        break;

        case 'x64':
        module.exports = require('./x86_64-apple-darwin/release/ipmb_js.node');
        break;

        default:
        throw new Error(`Unsupported platform: ${platform} ${arch}`);
    }
    break;

    case 'win32':
    switch (arch) {
        case 'x64':
        module.exports = require('./x86_64-pc-windows-msvc/release/ipmb_js.node');
        break;

        case 'ia32':
        module.exports = require('./i686-pc-windows-msvc/release/ipmb_js.node');
        break;

        default:
        throw new Error(`Unsupported platform: ${platform} ${arch}`);
    }
    break;

    default:
    throw new Error(`Unsupported platform: ${platform}-${arch}`);
}
