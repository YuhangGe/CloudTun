//
//  Test.swift
//  cloudtun_iOS
//
//  Created by YUHANG on 2025/9/20.
//

import Foundation
import NetworkExtension



class ViewController {
  
    func setupProxy() {
        NEAppProxyProviderManager.loadAllFromPreferences { managers, error in
            if let err = error {
                print("Load error: \(err)")
                return
            }
            
            let manager = managers?.first ?? NEAppProxyProviderManager()
            
            // 创建配置
            let proto = NETunnelProviderProtocol()
            proto.providerBundleIdentifier = "com.yourcompany.yourapp.SimpleHTTPProxyProvider"
            proto.serverAddress = "DummyServer"  // 仅用来显示，不实际连接
            
            manager.protocolConfiguration = proto
            manager.localizedDescription = "My HTTP Proxy"
            manager.isEnabled = true
            
            // 保存到系统
            manager.saveToPreferences { error in
                if let err = error {
                    print("Save error: \(err)")
                } else {
                    print("Saved successfully")
                    
                    // 激活
                    self.startProxy(manager: manager)
                }
            }
        }
    }
    
    func startProxy(manager: NEAppProxyProviderManager) {
        manager.loadFromPreferences { error in
            if let err = error {
                print("Load error: \(err)")
                return
            }
            
            do {
                try manager.connection.startVPNTunnel()
                print("Proxy started")
            } catch {
                print("Failed to start tunnel: \(error)")
            }
        }
    }
}
