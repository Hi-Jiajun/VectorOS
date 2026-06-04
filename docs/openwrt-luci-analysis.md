# OpenWrt LuCI Web Interface - Deep Analysis

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Directory Structure](#directory-structure)
3. [Core Framework (luci-base)](#core-framework)
4. [UCI System Integration](#uci-system)
5. [UBUS/ubus API Integration](#ubus-api)
6. [Form System](#form-system)
7. [View System](#view-system)
8. [i18n Internationalization](#i18n-system)
9. [Real-time Features](#real-time-features)
10. [Theme System](#theme-system)
11. [Application Structure](#application-structure)
12. [Key Applications Analysis](#key-applications)
13. [Lessons for VectorOS](#lessons-vectoros)

---

## 1. Architecture Overview

LuCI is OpenWrt's web interface, implementing a modern JavaScript-based architecture:

```
┌─────────────────────────────────────────────────────────────────────┐
│                         LuCI Architecture                          │
├─────────────────────────────────────────────────────────────────────┤
│  Browser (JavaScript ES6 Modules)                                  │
│    - luci.js (Core class system)                                   │
│    - form.js (Form builder)                                        │
│    - uci.js (UCI abstraction)                                      │
│    - rpc.js (JSON-RPC client)                                      │
│    - network.js, firewall.js (High-level APIs)                     │
├─────────────────────────────────────────────────────────────────────┤
│  Theme Layer (Bootstrap / Material / etc.)                         │
│    - menu.js (Navigation rendering)                                │
│    - header/footer templates (ucode)                               │
├─────────────────────────────────────────────────────────────────────┤
│  Server-Side (ucode - LuCI's template language)                   │
│    - dispatcher.uc (URL routing)                                   │
│    - runtime.uc (Page rendering)                                   │
│    - authplugins.uc (Authentication)                               │
├─────────────────────────────────────────────────────────────────────┤
│  rpcd (ubus JSON-RPC daemon)                                       │
│    - rpcd-mod-luci (C plugin - system/network info)                │
│    - App-specific uc plugins (ddns.uc, etc.)                       │
├─────────────────────────────────────────────────────────────────────┤
│  ubus (OpenWrt micro-bus daemon)                                   │
│    - uci (config management)                                       │
│    - network (network interfaces)                                  │
│    - system (system info)                                          │
│    - session (authentication)                                      │
├─────────────────────────────────────────────────────────────────────┤
│  UCI (/etc/config/*)                                               │
│    - Unified Configuration Interface                               │
│    - Key-value store with sections                                 │
└─────────────────────────────────────────────────────────────────────┘
```

**Key Design Principles:**
- Single Page Application (SPA) - JavaScript renders views, server provides data via JSON-RPC
- UCI as the configuration backend (all config in `/etc/config/`)
- ubus as the IPC/RPC mechanism
- ucode as server-side template language (replaced Lua)
- ACL-based permission control per application

---

## 2. Directory Structure

```
luci/
├── applications/          # Per-feature apps (luci-app-*)
│   ├── luci-app-firewall/
│   │   ├── htdocs/luci-static/resources/
│   │   │   ├── view/firewall/      # View JS modules
│   │   │   └── tools/firewall.js   # Shared tools
│   │   ├── root/usr/share/
│   │   │   ├── luci/menu.d/        # Menu registration (JSON)
│   │   │   ├── rpcd/acl.d/         # Permission definitions (JSON)
│   │   │   └── ucitrack/           # UCI change tracking
│   │   └── po/                     # Translations (*.po files)
│   └── luci-app-ddns/
│       └── root/usr/share/rpcd/ucode/  # Server-side UCODE plugins
│
├── modules/                # Core modules
│   ├── luci-base/          # Core framework
│   │   ├── htdocs/luci-static/resources/
│   │   │   ├── luci.js      # Core class system, helpers
│   │   │   ├── form.js      # Form widget system
│   │   │   ├── uci.js       # UCI client abstraction
│   │   │   ├── rpc.js       # JSON-RPC client
│   │   │   ├── network.js   # Network abstraction
│   │   │   ├── firewall.js  # Firewall abstraction
│   │   │   ├── ui.js        # UI components (tabs, modals)
│   │   │   └── dom.js       # DOM helpers
│   │   ├── ucode/           # Server-side code
│   │   │   ├── dispatcher.uc  # URL routing/dispatching
│   │   │   ├── runtime.uc     # Runtime environment
│   │   │   └── http.uc        # HTTP request handling
│   │   └── src/             # C utilities (po2lmo, jsmin)
│   │
│   ├── luci-mod-network/   # Network interfaces module
│   ├── luci-mod-system/    # System administration
│   ├── luci-mod-status/    # Status pages
│   └── luci-mod-dashboard/ # Dashboard overview
│
├── themes/                 # UI themes
│   ├── luci-theme-bootstrap/  # Default theme
│   ├── luci-theme-material/
│   └── luci-theme-openwrt/
│
├── libs/                   # Shared libraries
│   ├── rpcd-mod-luci/      # Core ubus plugin (C)
│   ├── luci-lib-ip/        # IP address utilities
│   └── luci-lib-json/
│
└── luci.mk                 # OpenWrt build system integration
```

---

## 3. Core Framework (luci-base)

### 3.1 Class System (`luci.js`)

LuCI uses a custom class system with prototypal inheritance:

```javascript
// Class creation
const MyClass = L.Class.extend({
    __name__: 'MyClass',
    
    __init__(arg1, arg2) {
        // Constructor
        this.value = arg1;
    },
    
    getValue() {
        return this.value;
    }
});

// Inheritance
const ChildClass = MyClass.extend({
    __name__: 'ChildClass',
    
    getValue() {
        return super.getValue() + ' (extended)';
    }
});

// Singleton
const Singleton = L.Class.singleton({
    // Only one instance ever created
});
```

**Key Base Classes:**
- `L.Class` - Base class for everything
- `L.baseclass` - Extended base with `extend()` method
- `L.rpc` - RPC client
- `L.uci` - UCI client
- `L.network` - Network abstraction
- `L.firewall` - Firewall abstraction

### 3.2 Module System

```javascript
// Require-style imports (resolved at runtime)
'require view';
'require rpc';
'require uci';
'require form';
'require network';
'require firewall';
'require tools.firewall as fwtool';
```

This is not standard ES modules - it's a LuCI-specific require system that loads modules from `luci-static/resources/`.

---

## 4. UCI System Integration

### 4.1 UCI Abstraction (`uci.js`)

The `uci` object provides a high-level API over ubus UCI calls:

```javascript
// Load configuration
await uci.load('network');

// Read values
const proto = uci.get('network', 'lan', 'proto');
const sections = uci.sections('network', 'interface');

// Write values (local changes)
uci.set('network', 'lan', 'proto', 'static');
uci.add('network', 'interface', 'wan');
uci.remove('network', 'wan', 'proto');

// Commit changes to disk
await uci.save();
await uci.apply();  // Triggers service restarts
```

### 4.2 UCI Configuration Model

UCI uses a simple key-value structure with named sections:

```
# /etc/config/network
config interface 'lan'
    option proto 'static'
    option ipaddr '192.168.1.1'
    option netmask '255.255.255.0'
    list dns '8.8.8.8'

config interface 'wan'
    option proto 'dhcp'
```

**Key Concepts:**
- Config file = `/etc/config/<name>`
- Section type = `interface`, `zone`, etc.
- Named sections = `'lan'`, `'wan'`
- Anonymous sections = auto-generated `cfg0a1b2c`
- Options = key-value pairs
- Lists = multiple values for same key

---

## 5. UBUS/ubus API Integration

### 5.1 RPC Client (`rpc.js`)

The frontend communicates with the backend via JSON-RPC over HTTP:

```javascript
// Declare an RPC method
const callSystemBoard = rpc.declare({
    object: 'system',        // ubus object
    method: 'board',         // ubus method
    expect: {                // Expected response structure
        model: '',
        system: '',
        kernel: ''
    }
});

// Call the method
const boardInfo = await callSystemBoard();
console.log(boardInfo.model);
```

### 5.2 Server-Side RPC Plugins

Plugins are written in **ucode** (LuCI's template language) and register ubus methods:

```javascript
// /usr/share/rpcd/ucode/ddns.uc
const methods = {
    get_services_status: {
        call: function() {
            // Read UCI, filesystem, run commands
            const uci = cursor();
            let res = {};
            
            uci.foreach('ddns', 'service', function(s) {
                res[s['.name']] = {
                    enabled: s['enabled'],
                    // ...
                };
            });
            
            return res;
        }
    }
};

return { 'luci.ddns': methods };
```

### 5.3 ACL (Access Control Lists)

Permissions are defined per-application:

```json
// /usr/share/rpcd/acl.d/luci-app-firewall.json
{
    "luci-app-firewall": {
        "description": "Grant access to firewall configuration",
        "read": {
            "ubus": {
                "file": ["read"],
                "luci": ["getConntrackHelpers"]
            },
            "uci": ["firewall"]
        },
        "write": {
            "ubus": {
                "file": ["write"]
            },
            "uci": ["firewall"]
        }
    }
}
```

### 5.4 Menu Registration

Applications register their menu entries:

```json
// /usr/share/luci/menu.d/luci-app-firewall.json
{
    "admin/network/firewall": {
        "title": "Firewall",
        "order": 60,
        "action": {
            "type": "alias",
            "path": "admin/network/firewall/zones"
        },
        "depends": {
            "acl": ["luci-app-firewall"],
            "uci": {"firewall": true}
        }
    },
    "admin/network/firewall/zones": {
        "title": "General Settings",
        "order": 10,
        "action": {
            "type": "view",
            "path": "firewall/zones"
        }
    }
}
```

---

## 6. Form System

### 6.1 Form Builder (`form.js`)

LuCI has a sophisticated form system for UCI configuration:

```javascript
return view.extend({
    render() {
        // Create a form map bound to UCI config
        const m = new form.Map('firewall', _('Firewall Settings'));
        
        // Create a section
        const s = m.section(form.TypedSection, 'zone', _('Zones'));
        s.anonymous = true;
        s.addremove = true;
        
        // Add options
        const o = s.option(form.Value, 'name', _('Name'));
        o.rmempty = false;
        o.datatype = 'maxlength(11)';
        
        const p = s.option(form.ListValue, 'input', _('Input policy'));
        p.value('REJECT', _('reject'));
        p.value('DROP', _('drop'));
        p.value('ACCEPT', _('accept'));
        
        const f = s.option(form.Flag, 'masq', _('Masquerading'));
        
        // Render the form
        return m.render();
    }
});
```

### 6.2 Form Widget Types

| Widget | Purpose | Example |
|--------|---------|---------|
| `form.Value` | Text input | IP address, hostname |
| `form.ListValue` | Dropdown select | Protocol, action |
| `form.MultiValue` | Multi-select | Allowed protocols |
| `form.Flag` | Checkbox | Enable/disable option |
| `form.DynamicList` | Tag-style list | DNS servers, IPs |
| `form.TypedSection` | Section with typed options | UCI sections |
| `form.GridSection` | Table-based sections | List of zones |
| `form.Section` | Abstract container | Custom layouts |

### 6.3 Data Flow

```
1. User loads page
   ↓
2. view.load() → RPC calls to fetch UCI data
   ↓
3. view.render() → Create form.Map, sections, options
   ↓
4. form.Map.render() → Generate HTML form
   ↓
5. User edits form
   ↓
6. "Save & Apply" clicked
   ↓
7. form.Map.save() → Collect changes
   ↓
8. uci.save() → RPC call to write UCI changes
   ↓
9. uci.apply() → Trigger service reloads
   ↓
10. Services restart with new config
```

---

## 7. View System

### 7.1 View Module Structure

Every page is a JavaScript module returning a view:

```javascript
'use strict';
'require view';
'require rpc';
'require uci';
'require form';

return view.extend({
    // Load data (called before render)
    load() {
        return Promise.all([
            rpc.someCall(),
            uci.load('config')
        ]);
    },
    
    // Render the page
    render(data) {
        // Return DOM elements
        return E('div', {}, [
            E('h2', {}, [_('Title')]),
            E('p', {}, [_('Description')])
        ]);
    },
    
    // Handle save (null = read-only view)
    handleSave: null,
    handleSaveApply: null,
    handleReset: null
});
```

### 7.2 DOM Helpers (`E()` function)

The `E()` function creates DOM elements:

```javascript
// Basic element
E('div', { 'class': 'container' }, ['Hello'])

// With attributes
E('a', { 'href': '/url', 'class': 'btn' }, ['Click me'])

// Nested elements
E('div', {}, [
    E('h1', {}, ['Title']),
    E('p', {}, ['Content'])
])

// With event handlers
E('button', {
    'click': function(ev) { /* handler */ }
}, ['Press'])
```

### 7.3 Polling System

For real-time updates:

```javascript
// In view.render()
poll.add(function() {
    // This runs periodically
    return rpc.someCall().then(function(data) {
        // Update DOM with new data
    });
}, 5);  // Every 5 seconds

// Clean up when leaving page
handleReset: function() {
    poll.remove(pollFn);
}
```

---

## 8. i18n Internationalization

### 8.1 Translation Function

All user-visible strings are wrapped with `_()`:

```javascript
// Simple translation
_('Firewall')           // "防火墙" in Chinese

// With context
_('Input', 'firewall')  // Different "Input" for firewall context

// With format args
_('Interface %s is up').format(ifname)
```

### 8.2 Translation Files

Translations use standard `.po` format:

```
# po/zh_Hans/firewall.po
msgid "Firewall"
msgstr "防火墙"

msgid "The firewall creates zones over your network interfaces"
msgstr "防火墙在网络接口上创建区域"
```

### 8.3 How It Works

1. **Template Extraction**: `.pot` files generated from source
2. **Translation**: Translators edit `.po` files per language
3. **Compilation**: `.po` → `.lmo` (binary format) via `po2lmo`
4. **Runtime**: `_()` looks up string in compiled catalog
5. **Distribution**: `lmo` files served as JS translations endpoint

### 8.4 Best Practices

```javascript
// DO: Wrap all user-visible strings
E('button', {}, [_('Save')])
E('h2', {}, [_('Configuration')])

// DON'T: Leave strings bare
E('button', {}, ['Save'])  // Won't be translated

// DON'T: Translate dynamically generated strings
_('Hello ' + name)  // Breaks translation

// DO: Use format placeholders
_('Hello %s').format(name)
```

---

## 9. Real-time Features

### 9.1 Statistics App (rrdtool)

The statistics app uses **rrdtool** for graphing:

```javascript
// rrdtool.js
const rrdtool = {
    // Render graph via rrdtool command line
    render(plugin, instance, is_index, host, timespan, width) {
        const cmdline = [
            'graph', '-', '-a', 'PNG',
            '-s', 'NOW-' + timespan,
            '-e', 'NOW-15',
            '-w', width,
            '-h', this.opts.height
        ];
        
        // Add DEF, CDEF, LINE, AREA statements
        for (let d of definition) {
            cmdline.push(d);
        }
        
        // Execute rrdtool and return PNG blob
        return fs.exec_direct('/usr/bin/rrdtool', cmdline, 'blob');
    }
};
```

**Architecture:**
- **collectd** → Collects metrics → Writes `.rrd` files
- **rrdtool** → Reads `.rrd` → Generates PNG graphs
- **LuCI** → Calls rrdtool → Displays PNG images
- **Polling** → Periodically refreshes graphs

### 9.2 Dashboard Updates

```javascript
// Dashboard uses polling for live updates
startPolling(includes, containers) {
    const step = () => {
        return network.flushCache().then(() => {
            return invokeIncludesLoad(includes);
        }).then(results => {
            // Re-render each include with fresh data
            for (let i = 0; i < includes.length; i++) {
                content = includes[i].render(results[i]);
                dom.content(containers[i], content);
            }
        });
    };
    
    // Initial render + periodic refresh
    return step().then(() => {
        poll.add(step);  // Default: every 5 seconds
    });
}
```

### 9.3 Network Status Updates

```javascript
// Network interface status with polling
const callNetworkDeviceStatus = rpc.declare({
    object: 'network.device',
    method: 'status',
    params: ['name'],
    expect: { '': {} }
});

// In view
load() {
    return callNetworkDeviceStatus('eth0');
}

render(data) {
    // Render status bar, uptime, traffic counters
    // Polling updates this periodically
}
```

---

## 10. Theme System

### 10.1 Theme Structure

```
themes/luci-theme-bootstrap/
├── htdocs/luci-static/
│   ├── bootstrap/
│   │   ├── cascade.css      # Main stylesheet
│   │   ├── mobile.css       # Mobile responsive
│   │   └── logo.svg         # Logo
│   └── resources/
│       ├── menu-bootstrap.js  # Menu rendering logic
│       └── view/bootstrap/
│           └── sysauth.js     # Login page
├── ucode/template/themes/bootstrap/
│   ├── header.ut             # Page header template
│   ├── footer.ut             # Page footer template
│   └── sysauth.ut            # Login template
└── root/etc/uci-defaults/
    └── 30_luci-theme-bootstrap  # Theme registration
```

### 10.2 Menu Rendering

The theme's `menu-bootstrap.js` renders the navigation:

```javascript
return baseclass.extend({
    __init__() {
        ui.menu.load().then((tree) => this.render(tree));
    },
    
    render(tree) {
        this.renderModeMenu(tree);      // Top-level mode tabs
        
        if (L.env.dispatchpath.length >= 3) {
            this.renderTabMenu(node, url);  // Sub-tabs
        }
    },
    
    renderModeMenu(tree) {
        const children = ui.menu.getChildren(tree);
        
        children.forEach((child) => {
            ul.appendChild(E('li', {}, [
                E('a', { 'href': L.url(child.name) }, [_(child.title)])
            ]));
            
            if (isActive)
                this.renderMainMenu(child, child.name);
        });
    },
    
    renderMainMenu(tree, url, level) {
        // Renders dropdown menus
        // Handles nested menu items
    }
});
```

### 10.3 Template System (ucode)

Header/footer use ucode templates:

```html
<!-- header.ut -->
<html lang="{{ dispatcher.lang }}">
<head>
    <title>{{ striptags(boardinfo.hostname) }}</title>
    <link rel="stylesheet" href="{{ media }}/cascade.css">
</head>
<body>
    <header>
        <a class="brand" href="/">{{ striptags(boardinfo.hostname) }}</a>
        <ul class="nav" id="topmenu"></ul>
    </header>
    
    <div id="tabmenu" style="display:none"></div>
    
    <div id="maincontent" class="container">
        {% if (getuid() == 0): %}
            <div class="alert-message warning">
                {{ _('No password set!') }}
            </div>
        {% endif %}
```

---

## 11. Application Structure

### 11.1 Standard App Layout

```
luci-app-<name>/
├── Makefile                    # OpenWrt package definition
├── htdocs/luci-static/resources/
│   ├── view/<name>/           # View JavaScript modules
│   │   ├── overview.js        # Main view
│   │   ├── settings.js        # Settings view
│   │   └── status.js          # Status view
│   └── tools/<name>.js        # Shared tools/libraries
├── root/usr/share/
│   ├── luci/menu.d/           # Menu registration (JSON)
│   │   └── luci-app-<name>.json
│   ├── rpcd/acl.d/            # ACL definitions (JSON)
│   │   └── luci-app-<name>.json
│   └── rpcd/ucode/            # Server-side plugins (optional)
│       └── <name>.uc
└── po/                        # Translations
    ├── templates/<name>.pot    # Translation template
    ├── zh_Hans/<name>.po      # Chinese (Simplified)
    ├── en/<name>.po           # English
    └── ...
```

### 11.2 Menu Registration Example

```json
{
    "admin/services/ddns": {
        "title": "Dynamic DNS",
        "order": 59,
        "action": {
            "type": "view",
            "path": "ddns/overview"
        },
        "depends": {
            "acl": ["luci-app-ddns"]
        }
    }
}
```

**URL Structure:** `/#/admin/services/ddns`

### 11.3 ACL Registration Example

```json
{
    "luci-app-ddns": {
        "description": "Grant access to DDNS configuration",
        "read": {
            "ubus": {
                "file": ["read"],
                "uci": ["ddns"]
            }
        },
        "write": {
            "ubus": {
                "file": ["write"]
            },
            "uci": ["ddns"]
        }
    }
}
```

---

## 12. Key Applications Analysis

### 12.1 luci-app-firewall

**Architecture:**
- **Views:** zones.js, forwards.js, rules.js, snats.js, ipsets.js, custom.js
- **Tools:** firewall.js (shared utilities)
- **Backend:** UCI config at `/etc/config/firewall`

**Zone Management (zones.js):**

```javascript
return view.extend({
    load() {
        return Promise.all([
            this.callConntrackHelpers(),
            firewall.getDefaults()
        ]);
    },
    
    render([ctHelpers, fwDefaults]) {
        const m = new form.Map('firewall', _('Firewall - Zone Settings'));
        
        // General settings section
        s = m.section(form.TypedSection, 'defaults', _('General Settings'));
        s.option(form.Flag, 'synflood_protect', _('Enable SYN-flood protection'));
        s.option(form.Flag, 'drop_invalid', _('Drop invalid packets'));
        
        // Zone list
        s = m.section(form.GridSection, 'zone', _('Zones'));
        s.addremove = true;
        s.anonymous = true;
        s.sortable = true;
        
        // Zone options with tabs
        s.tab('general', _('General Settings'));
        s.tab('advanced', _('Advanced Settings'));
        
        o = s.taboption('general', form.Value, 'name', _('Name'));
        o = s.taboption('general', widgets.NetworkSelect, 'network', _('Covered networks'));
        o.multiple = true;
        
        return m.render();
    }
});
```

**Key Patterns:**
- Uses `firewall.js` library for firewall abstraction
- Custom widgets: `widgets.ZoneSelect`, `widgets.NetworkSelect`
- Complex data transformations in `cfgvalue` and `write` methods
- Zone-based security model

### 12.2 luci-app-ddns

**Architecture:**
- **Views:** overview.js
- **Backend:** ddns.uc (ucode plugin) + shell scripts
- **Data:** UCI config + runtime state files

**Status Display:**

```javascript
return view.extend({
    // RPC declarations
    callDDnsGetStatus: rpc.declare({
        object: 'luci.ddns',
        method: 'get_ddns_state',
        expect: {}
    }),
    
    callDDnsGetServicesStatus: rpc.declare({
        object: 'luci.ddns',
        method: 'get_services_status',
        expect: {}
    }),
    
    load() {
        return Promise.all([
            this.callDDnsGetStatus(),
            this.callDDnsGetServicesStatus(),
            this.callDDnsGetEnv(),
            this.callGenServiceList()
        ]);
    },
    
    render([status, servicesStatus, env, services]) {
        // Render service table with status
        // Enable/disable buttons
        // Log viewer
        // Service configuration forms
    }
});
```

**Key Patterns:**
- Server-side ucode plugin for shell script integration
- Runtime state from files (`/var/run/ddns/`)
- Service list from filesystem (`/usr/share/ddns/`)
- Environment detection (curl, wget, SSL support)

### 12.3 luci-app-statistics

**Architecture:**
- **Views:** graphs.js, collectd.js, plugins/*.js
- **Backend:** collectd → rrdtool → PNG graphs
- **Data:** RRD files in `/tmp/rrd/`

**Graph Rendering:**

```javascript
// rrdtool.js
render(plugin, plugin_instance, is_index, hostname, timespan, width, height, cache) {
    const def = graphdefs[plugin];
    
    if (def && typeof(def.rrdargs) == 'function') {
        // Get diagram definitions from plugin
        const optlist = this._forcelol(def.rrdargs(
            this, hostname, plugin, plugin_instance, null, is_index
        ));
        
        for (let opt of optlist) {
            // Generate RRDtool command line
            const diagrams = this._generic(opt, hostname, plugin, plugin_instance);
            
            for (let diagram of diagrams) {
                // Execute rrdtool to generate PNG
                pngs.push(this._rrdtool(diagram, null, timespan, width, height, cache));
            }
        }
    }
    
    return Promise.all(pngs);
}
```

**Plugin System:**
- Each plugin type (cpu, memory, network, etc.) has a definition file
- Definitions specify how to query RRD data
- Graph rendering is delegated to rrdtool

### 12.4 luci-mod-network

**Architecture:**
- **Views:** interfaces.js, wireless.js, dhcp.js, dns.js, routes.js
- **Backend:** network.js abstraction over ubus network API
- **Data:** UCI `/etc/config/network`, `/etc/config/wireless`

**Interface Management:**

```javascript
return view.extend({
    load() {
        return Promise.all([
            network.getNetworks(),
            network.getDevices(),
            network.getWifiNetworks()
        ]);
    },
    
    render([networks, devices, wifinets]) {
        const m = new form.Map('network', _('Interfaces'));
        
        // Interface table
        const s = m.section(form.GridSection, 'interface', _('Interfaces'));
        s.sortable = true;
        s.anonymous = true;
        
        // Real-time status display
        o = s.option(form.DummyValue, '_ifacestat', _('Status'));
        o.width = '20%';
        o.modalonly = false;
        o.render = function(section_id) {
            // Render live status with up/down, IP, traffic
            return render_status(E('div'), ifc, false);
        };
        
        return m.render();
    }
});
```

---

## 13. Lessons for VectorOS

### 13.1 Architecture Comparison

| Aspect | LuCI (OpenWrt) | VectorOS (Current) |
|--------|----------------|---------------------|
| Backend | ubus (C daemon) | Rust (Axum HTTP) |
| Config | UCI (key-value files) | TOML + UCI |
| RPC | JSON-RPC over HTTP | REST API |
| Frontend | JavaScript (ES6 modules) | Svelte |
| Templates | ucode | Svelte components |
| i18n | PO files | To be implemented |
| Real-time | Polling + rrdtool | WebSocket planned |

### 13.2 Key Patterns to Adopt

#### 1. Modular Application Structure

```javascript
// LuCI pattern: Each app is self-contained
luci-app-firewall/
├── views/           # Frontend
├── menu.d/          # Menu registration
├── acl.d/           # Permissions
├── rpcd/ucode/      # Backend plugins
└── po/              # Translations
```

**For VectorOS:**
```
frontend/src/routes/
├── firewall/
│   ├── +page.svelte
│   ├── +layout.svelte
│   └── stores.ts
├── network/
└── ddns/
```

#### 2. Form Builder Pattern

LuCI's form system generates UCI forms declaratively:

```javascript
const m = new form.Map('firewall', _('Firewall'));
const s = m.section(form.TypedSection, 'zone', _('Zones'));
s.option(form.Value, 'name', _('Name'));
s.option(form.ListValue, 'input', _('Input policy'));
```

**For VectorOS - Svelte Equivalent:**

```svelte
<script>
  import { Form, FormSection, FormValue, FormList, FormFlag } from '$lib/components';
</script>

<Form config="firewall" title="Firewall">
  <FormSection type="zone" title="Zones" addable removable>
    <FormValue option="name" label="Name" required />
    <FormList option="input" label="Input Policy">
      <option value="REJECT">Reject</option>
      <option value="DROP">Drop</option>
      <option value="ACCEPT">Accept</option>
    </FormList>
  </FormSection>
</Form>
```

#### 3. Real-time Updates

LuCI uses polling for live data:

```javascript
poll.add(function() {
    return callNetworkDeviceStatus('eth0').then(function(data) {
        // Update DOM
    });
}, 5);  // 5 second interval
```

**For VectorOS - WebSocket/SSE:**

```typescript
// Using Svelte stores with WebSocket
import { writable } from 'svelte/store';

export const interfaceStatus = writable({});

// WebSocket connection
const ws = new WebSocket('ws://router/api/ws/interfaces');
ws.onmessage = (event) => {
    interfaceStatus.set(JSON.parse(event.data));
};

// In component
<script>
  import { interfaceStatus } from '$lib/stores';
</script>

{#if $interfaceStatus.up}
  <span class="badge success">Connected</span>
{/if}
```

#### 4. i18n System

LuCI uses standard PO files:

```po
msgid "Firewall"
msgstr "防火墙"
```

**For VectorOS - i18next (Svelte standard):**

```typescript
// src/lib/i18n.ts
import i18n from 'i18next';
import { init } from 'svelte-i18n';

init({
    fallbackLocale: 'en',
    initialLocale: 'zh',
    resources: {
        en: { translation: { firewall: 'Firewall' } },
        zh: { translation: { firewall: '防火墙' } }
    }
});
```

```svelte
<script>
  import { _ } from 'svelte-i18n';
</script>

<h2>{$_('firewall')}</h2>
```

#### 5. ACL/Permissions

LuCI's ACL model:

```json
{
    "luci-app-firewall": {
        "read": { "uci": ["firewall"] },
        "write": { "uci": ["firewall"] }
    }
}
```

**For VectorOS:**

```rust
// In Axum middleware
async fn check_permission(
    State(state): State<AppState>,
    Path(app): Path<String>,
    user: User,
) -> Result<(), StatusCode> {
    if state.acl.check(&user, &app, "read") {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}
```

### 13.3 Features to Implement

Based on LuCI analysis, VectorOS should prioritize:

#### Phase 1: Core UI Framework
1. **Form Builder Component** - Reusable form components for config
2. **UCI-like Config Store** - Centralized config management
3. **Polling/Subscription System** - Real-time data updates
4. **Modal/Dialog System** - For confirmations and edits
5. **Table Components** - For lists of interfaces, rules, etc.

#### Phase 2: Essential Apps
1. **Dashboard** - System overview with live stats
2. **Network Interfaces** - WAN/LAN configuration
3. **Firewall** - Zone-based rules (like luci-app-firewall)
4. **DHCP Leases** - View and manage DHCP

#### Phase 3: Advanced Features
1. **i18n System** - Multi-language support
2. **Statistics/Monitoring** - Graphs (consider Chart.js instead of rrdtool)
3. **DDNS Configuration** - Dynamic DNS setup
4. **VPN Configuration** - OpenVPN/WireGuard

#### Phase 4: Polish
1. **Themes** - Multiple theme support
2. **Plugin System** - Extensible architecture
3. **Mobile Responsive** - Better mobile experience
4. **Accessibility** - ARIA labels, keyboard navigation

### 13.4 Technical Recommendations

#### Use TypeScript Throughout

LuCI uses plain JavaScript, but TypeScript provides:
- Type safety for API responses
- Better IDE support
- Self-documenting code

```typescript
interface FirewallZone {
    name: string;
    input: 'ACCEPT' | 'REJECT' | 'DROP';
    output: 'ACCEPT' | 'REJECT' | 'DROP';
    forward: 'ACCEPT' | 'REJECT' | 'DROP';
    masq?: boolean;
    networks?: string[];
}
```

#### Adopt Svelte Stores for State

LuCI uses global `uci.load()` calls. Svelte stores are cleaner:

```typescript
// src/lib/stores/firewall.ts
import { writable, derived } from 'svelte/store';
import { api } from '$lib/api';

export const zones = writable<FirewallZone[]>([]);
export const selectedZone = writable<string | null>(null);

export async function loadZones() {
    const response = await api.get('/api/firewall/zones');
    zones.set(response.data);
}
```

#### Use WebSockets for Real-time

LuCI polls every 5 seconds. WebSockets are more efficient:

```typescript
// Real-time interface status
const ws = new WebSocket('ws://router/api/ws');

ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    
    switch (msg.type) {
        case 'interface_status':
            interfaceStore.update(msg.data);
            break;
        case 'dhcp_lease':
            dhcpStore.addLease(msg.data);
            break;
    }
};
```

#### Implement Proper Error Handling

LuCI has minimal error handling. VectorOS should be better:

```typescript
// In API client
async function fetchConfig(section: string) {
    try {
        const response = await api.get(`/api/config/${section}`);
        return { data: response.data, error: null };
    } catch (err) {
        console.error(`Failed to load ${section}:`, err);
        return { data: null, error: err.message };
    }
}
```

### 13.5 Code Examples to Port

#### LuCI Firewall Zone Editor → VectorOS

**LuCI Version (zones.js):**
```javascript
return view.extend({
    renderZones([ctHelpers, fwDefaults]) {
        const m = new form.Map('firewall', _('Firewall - Zone Settings'));
        
        s = m.section(form.GridSection, 'zone', _('Zones'));
        s.addremove = true;
        s.anonymous = true;
        
        o = s.taboption('general', form.Value, 'name', _('Name'));
        o = s.taboption('general', form.ListValue, 'input', _('Input'));
        o.value('REJECT', _('reject'));
        o.value('DROP', _('drop'));
        o.value('ACCEPT', _('accept'));
        
        return m.render();
    }
});
```

**VectorOS Equivalent (Svelte):**
```svelte
<!-- src/routes/firewall/zones/+page.svelte -->
<script lang="ts">
    import { onMount } from 'svelte';
    import { firewallZones, addZone, removeZone } from '$lib/stores/firewall';
    import { Form, FormSection, FormList, FormValue } from '$lib/components/form';
    import { Modal, ConfirmDialog } from '$lib/components/ui';
    
    let showModal = false;
    let editingZone: FirewallZone | null = null;
    
    onMount(() => {
        firewallZones.load();
    });
    
    function handleAdd() {
        editingZone = null;
        showModal = true;
    }
    
    function handleEdit(zone: FirewallZone) {
        editingZone = zone;
        showModal = true;
    }
    
    async function handleSave(zone: FirewallZone) {
        if (editingZone) {
            await firewallZones.update(zone);
        } else {
            await firewallZones.add(zone);
        }
        showModal = false;
    }
</script>

<div class="page-header">
    <h1>Firewall Zones</h1>
    <button class="btn btn-primary" on:click={handleAdd}>
        Add Zone
    </button>
</div>

<div class="zone-list">
    {#each $firewallZones as zone (zone.name)}
        <div class="zone-card">
            <div class="zone-header">
                <h3>{zone.name}</h3>
                <div class="zone-actions">
                    <button on:click={() => handleEdit(zone)}>Edit</button>
                    <button on:click={() => removeZone(zone.name)}>Delete</button>
                </div>
            </div>
            <div class="zone-policies">
                <span class="badge">Input: {zone.input}</span>
                <span class="badge">Output: {zone.output}</span>
                <span class="badge">Forward: {zone.forward}</span>
            </div>
            <div class="zone-networks">
                Networks: {zone.networks?.join(', ') || 'None'}
            </div>
        </div>
    {/each}
</div>

{#if showModal}
    <Modal on:close={() => showModal = false}>
        <Form 
            title={editingZone ? 'Edit Zone' : 'Add Zone'}
            on:submit={handleSave}
        >
            <FormValue 
                option="name" 
                label="Name" 
                value={editingZone?.name || ''}
                required 
            />
            <FormList 
                option="input" 
                label="Input Policy"
                value={editingZone?.input || 'ACCEPT'}
            >
                <option value="REJECT">Reject</option>
                <option value="DROP">Drop</option>
                <option value="ACCEPT">Accept</option>
            </FormList>
            <!-- More fields... -->
        </Form>
    </Modal>
{/if}
```

---

## 14. Summary

### LuCI Strengths
1. **Mature UCI integration** - Decades of refinement
2. **Comprehensive app ecosystem** - 100+ applications
3. **Powerful form system** - Declarative form generation
4. **Real-time updates** - Polling and live data
5. **Internationalization** - Full i18n support
6. **Theme system** - Customizable appearance
7. **ACL system** - Fine-grained permissions

### LuCI Weaknesses (for VectorOS to avoid)
1. **Complex JavaScript** - No modules, no types
2. **Server-side rendering** - Ucode templates mix concerns
3. **Polling-based** - Not efficient for high-frequency updates
4. **No WebSocket** - Missing real-time push
5. **Limited error handling** - Silent failures common
6. **Tight UCI coupling** - Hard to use other backends

### VectorOS Advantages
1. **Rust backend** - Memory safe, fast, modern
2. **Svelte frontend** - Reactive, compiled, small bundles
3. **TypeScript** - Type safety, better DX
4. **REST + WebSocket** - Modern API patterns
5. **VPP integration** - High-performance data plane
6. **Clean architecture** - Separation of concerns

### Implementation Priority

```
Week 1-2: Core Framework
├── Form components
├── Config store
├── API client
└── Basic layout

Week 3-4: Dashboard
├── System overview
├── Interface status
├── Traffic stats
└── Live updates

Week 5-6: Network Config
├── Interface management
├── DHCP settings
├── DNS settings
└── VLAN support

Week 7-8: Firewall
├── Zone management
├── Rule editor
├── NAT configuration
└── Logging

Week 9-10: Advanced Features
├── i18n system
├── Statistics graphs
├── VPN configuration
└── Plugin system
```

---

## Appendix: Key File Paths in LuCI

### Core Framework
- `/luci/modules/luci-base/htdocs/luci-static/resources/luci.js` - Core class system
- `/luci/modules/luci-base/htdocs/luci-static/resources/form.js` - Form builder
- `/luci/modules/luci-base/htdocs/luci-static/resources/uci.js` - UCI client
- `/luci/modules/luci-base/htdocs/luci-static/resources/rpc.js` - RPC client
- `/luci/modules/luci-base/htdocs/luci-static/resources/network.js` - Network API
- `/luci/modules/luci-base/htdocs/luci-static/resources/firewall.js` - Firewall API

### Server-Side
- `/luci/modules/luci-base/ucode/dispatcher.uc` - URL routing
- `/luci/modules/luci-base/ucode/runtime.uc` - Page rendering
- `/luci/libs/rpcd-mod-luci/src/luci.c` - Core ubus plugin (C)

### Example Applications
- `/luci/applications/luci-app-firewall/` - Firewall management
- `/luci/applications/luci-app-ddns/` - Dynamic DNS
- `/luci/applications/luci-app-statistics/` - System statistics
- `/luci/modules/luci-mod-network/` - Network interfaces
- `/luci/modules/luci-mod-dashboard/` - Dashboard overview

### Themes
- `/luci/themes/luci-theme-bootstrap/` - Default Bootstrap theme
- `/luci/themes/luci-theme-bootstrap/htdocs/luci-static/resources/menu-bootstrap.js` - Menu rendering
- `/luci/themes/luci-theme-bootstrap/ucode/template/themes/bootstrap/header.ut` - Page header

---

*Analysis completed: 2026-06-04*
*Repository: https://github.com/openwrt/luci*
*Cloned to: /home/hiliang/Github/luci*
