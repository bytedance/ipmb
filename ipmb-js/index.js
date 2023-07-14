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

const raw_join = module.exports.join;

module.exports.join = (options) => {
    let ep = raw_join(options);
    let rx = ep.receiver;

    ep.receiver = {
        recv: (timeout) => {
            let message = rx.tryRecv();
            if (message) {
                return Promise.resolve(message);
            }

            if (timeout) {
                return rx.recv(timeout);
            }

            return new Promise(async (resolve, reject) => {
                while (true) {
                    try {
                        return resolve(await rx.recv(1000));
                    } catch (e) {
                        if (!e.toString().includes("timed out waiting on channel")) {
                            return reject(e);
                        }
                    }
                }
            });
        },
        close: () => {
            rx.close();
        },
    };

    return ep;
}
