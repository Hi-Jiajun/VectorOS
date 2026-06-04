# OPNsense Architecture Analysis

Deep analysis of OPNsense core source code for design patterns applicable to VectorOS.

## 1. Architecture Overview

### 1.1 Technology Stack

| Layer | Technology |
|-------|-----------|
| Data Plane | FreeBSD kernel `pf` packet filter |
| Control Plane (Frontend) | PHP + Phalcon MVC framework (Volt templates) |
| Control Plane (Backend) | Python configd service (Unix socket IPC) |
| Web UI | jQuery + Bootstrap 3 + D3.js/NVD3 charts |
| Config Storage | Single XML file (`config.xml`) + MVC model cache |
| VPN | WireGuard, OpenVPN, IPsec (strongSwan), L2TP, PPPoE |
| DNS | Unbound resolver, dnsmasq forwarder |
| DHCP | Kea DHCP, ISC DHCP (legacy) |
| IDS/IPS | Suricata integration |
| Traffic Shaping | FreeBSD `dn pipes` + `dn queues` (ALTQ replacement) |
| Routing | FRRouting (BGP/OSPF) via FPM |
| HA | CARP + pfsync + XML-RPC config sync |

### 1.2 Code Organization

OPNsense uses a **two-tier architecture** with clear separation between frontend (PHP/Phalcon MVC) and backend (Python configd):

```
src/
  opnsense/
    mvc/                           # Phalcon MVC application
      app/
        config/                    # App configuration
        controllers/OPNsense/      # API + UI controllers (25 modules)
          Firewall/                # Firewall rules, aliases, NAT
            Api/                   # REST API controllers
            forms/                 # Form definitions
          Wireguard/               # WireGuard VPN
          TrafficShaper/           # QoS / traffic shaping
          IDS/                     # Intrusion detection (Suricata)
          OpenVPN/                 # OpenVPN
          IPsec/                   # IPsec VPN
          Dnsmasq/                 # DNS forwarder
          Unbound/                 # DNS resolver
          Kea/                     # DHCP (Kea)
          Interfaces/              # Network interfaces
          Routes/                  # Routing table
          ...
        models/OPNsense/           # Data models (XML + PHP pairs)
          Base/FieldTypes/         # 36 field type classes
          Firewall/                # Filter, Alias, Group, Category, NAT
          Wireguard/               # General, Server, Client
          TrafficShaper/           # Pipes, Queues, Rules
          IDS/                     # Rules, Policies, Settings
          OpenVPN/                 # Instances, Overwrites, StaticKeys
          ...
        views/OPNsense/            # Volt templates (UI rendering)
        library/OPNsense/Base/     # UIModelGrid, ViewTranslator
    service/                       # Python configd backend
      configd.py                   # Main daemon (Unix socket server)
      configd_ctl.py               # CLI client tool
      modules/                     # Handler modules (processhandler, etc.)
      conf/
        configd.conf               # Daemon configuration
        actions_service.conf       # Service start/stop/restart actions
        actions.d/                 # 30 action config files per module
          actions_filter.conf      # Firewall reload, rule stats, table ops
          actions_wireguard.conf   # WireGuard start/stop/configure
          actions_ids.conf         # IDS/IPS management
          actions_shaper.conf      # Traffic shaper actions
          ...
    scripts/                       # Backend helper scripts (Python/Bash)
    www/                           # Legacy PHP pages (procedural)
  www/                             # Legacy web UI (pre-MVC)
    javascript/                    # opnsense_legacy.js
    *.php                          # Legacy PHP pages (firewall_nat_out.php, etc.)
```

### 1.3 Key Design Pattern: XML-Defined Models + PHP Validation

OPNsense's MVC approach is **model-driven**: each configuration domain is defined by a paired XML schema + PHP model class. The XML defines field types, constraints, and relationships. The PHP class provides custom validation logic.

```
┌─────────────────────────────────────────────┐
│  XML Model Definition (*.xml)               │
│  - Field types (BooleanField, NetworkField) │
│  - Constraints (Required, Unique, Regex)    │
│  - Relationships (ModelRelationField)       │
│  - Version + migration prefix               │
└─────────────────┬───────────────────────────┘
                  │ parses into
┌─────────────────▼───────────────────────────┐
│  PHP Model Class (*.php)                    │
│  extends BaseModel                          │
│  - performValidation() custom rules         │
│  - Helper methods (whereUsed, refactor)     │
│  - Rollback support                         │
└─────────────────┬───────────────────────────┘
                  │ exposes via
┌─────────────────▼───────────────────────────┐
│  API Controller (Api/*.php)                 │
│  extends ApiMutableModelControllerBase      │
│  - searchItemAction()                       │
│  - addItemAction() / setItemAction()        │
│  - getItemAction() / delItemAction()        │
│  - toggleItemAction()                       │
└─────────────────┬───────────────────────────┘
                  │ delegates to
┌─────────────────▼───────────────────────────┐
│  configd Backend (Python)                   │
│  - template reload (Jinja2)                 │
│  - service start/stop/restart               │
│  - firewall rule generation                 │
│  - Suricata management                      │
└─────────────────────────────────────────────┘
```

### 1.4 Frontend vs Backend Separation

The critical architectural insight is the **frontend/backend split**:

- **Frontend (PHP/Phalcon)**: Handles UI rendering, form validation, config CRUD via models. Communicates with backend via Unix domain socket (`/var/run/configd.socket`).
- **Backend (Python configd)**: Handles service management, config file generation, daemon control. Actions are defined in `.conf` files with `type: script|script_output|stream_output|inline`.

This separation means:
1. The frontend never directly manages daemons
2. Configuration changes are validated by the model, then applied by configd
3. The backend can restart services independently
4. Template generation (Jinja2) happens in configd, not the frontend

## 2. Configuration Model

### 2.1 XML-Based Configuration (`config.xml`)

The entire system state lives in a single XML file. The MVC models mount into this XML tree:

```xml
<opnsense>
  <OPNsense>
    <Firewall>
      <Filter>
        <rules>
          <rule uuid="...">
            <enabled>1</enabled>
            <sequence>1</sequence>
            <action>pass</action>
            <interface>lan</interface>
            <direction>in</direction>
            <ipprotocol>inet</ipprotocol>
            <protocol>tcp</protocol>
            <source_net>any</source_net>
            <source_port></source_port>
            <destination_net>192.168.1.0/24</destination_net>
            <destination_port>443</destination_port>
            <statetype>keep</statetype>
            <description>Allow HTTPS to LAN</description>
          </rule>
        </rules>
        <snatrules>...</snatrules>
        <npt>...</npt>
        <onetoone>...</onetoone>
      </Filter>
      <Alias>
        <aliases>
          <alias uuid="...">
            <enabled>1</enabled>
            <name>myaliases</name>
            <type>host</type>
            <content>192.168.1.10
192.168.1.11</content>
            <description>Internal servers</description>
          </alias>
        </aliases>
      </Alias>
    </Firewall>
    <Wireguard>
      <Server>...</Server>
      <Client>...</Client>
    </Wireguard>
    <TrafficShaper>
      <pipes>...</pipes>
      <queues>...</queues>
      <rules>...</rules>
    </TrafficShaper>
    <IDS>
      <general>...</general>
      <rules>...</rules>
      <policies>...</policies>
    </IDS>
  </OPNsense>
  <filter>           <!-- Legacy pfSense-compatible section -->
    <rule>...</rule>
  </filter>
  <nat>
    <rule>...</rule>
  </nat>
</opnsense>
```

### 2.2 Model Mount Points

Each model declares a `mount` path in its XML:

```xml
<mount>//OPNsense/Firewall/Filter</mount>
```

The double `//` prefix indicates the root of the config tree. The model parser reads the XML definition, then looks for configuration data at that mount path in `config.xml`.

### 2.3 Versioning and Migrations

Models declare a version and migration prefix:

```xml
<version>1.0.4</version>
<migration_prefix>MFP</migration_prefix>
```

When the version changes, migration scripts in the `Migrations/` directory run automatically to upgrade the configuration. The `BaseModel::runMigrations()` method handles this.

### 2.4 Field Type System

The model system provides 36 field types, each with built-in validation:

| Category | Field Types |
|----------|------------|
| **Basic** | TextField, DescriptionField, BooleanField, IntegerField, NumericField |
| **Network** | NetworkField, NetworkAliasField, IPPortField, HostnameField, MacAddressField |
| **Selection** | OptionField, InterfaceField, ProtocolField, PortField, CountryField |
| **Advanced** | Base64Field, UrlField, EmailField, JsonField, JsonKeyValueStoreField |
| **Relations** | ModelRelationField, ArrayField, ContainerField, CSVListField |
| **System** | AutoNumberField, UniqueIdField, CertificateField, VirtualIPField |
| **Legacy** | LegacyLinkField, UpdateOnlyTextField, ConfigdActionsField |

Key field type features:
- **`<Required>Y</Required>`** - Mandatory field
- **`<Mask>/regex/</Mask>`** - Regex validation
- **`<MinimumValue>` / `<MaximumValue>`** - Range constraints
- **`<Default>`** - Default value
- **`<Multiple>Y</Multiple>`** - Multi-select
- **`<volatile>Y</volatile>`** - Runtime-only (not persisted)
- **Constraints**: `UniqueConstraint`, `SetIfConstraint`, `DependConstraint`, `SingleSelectConstraint`

## 3. Firewall Rule Model

### 3.1 Rule Structure

Each firewall rule contains these fields (from `Filter.xml`):

```xml
<rule type=".\FilterRuleField">
  <!-- Core fields -->
  <enabled type="BooleanField">1</enabled>
  <sequence type=".\FilterSequenceField">1</sequence>
  <action type="OptionField">pass|block|reject</action>
  <quick type="BooleanField">1</quick>           <!-- Apply rule immediately -->

  <!-- Interface matching -->
  <interface type="InterfaceField">              <!-- Multiple interfaces supported -->
    <Multiple>Y</Multiple>
  </interface>
  <interfacenot type="BooleanField">0</interfacenot>
  <direction type="OptionField">in|out|any</direction>

  <!-- Protocol -->
  <ipprotocol type="OptionField">inet|inet6|inet46</ipprotocol>
  <protocol type="ProtocolField">any|tcp|udp|icmp|...</protocol>
  <icmptype type="OptionField"><Multiple>Y</Multiple></icmptype>
  <icmp6type type="OptionField"><Multiple>Y</Multiple></icmp6type>

  <!-- Source -->
  <source_net type="NetworkAliasField">any</source_net>
  <source_not type="BooleanField">0</source_not>
  <source_port type="PortField"/>

  <!-- Destination -->
  <destination_net type="NetworkAliasField">any</destination_net>
  <destination_not type="BooleanField">0</destination_not>
  <destination_port type="PortField"/>

  <!-- State management -->
  <statetype type="OptionField">
    keep|sloppy|modulate|synproxy|none
  </statetype>
  <state-policy type="OptionField">if-bound|floating</state-policy>
  <statetimeout type="IntegerField"/>

  <!-- Gateway / routing -->
  <gateway type="JsonKeyValueStoreField"/>       <!-- Populated by configd -->
  <replyto type="JsonKeyValueStoreField"/>
  <divert-to type="JsonKeyValueStoreField"/>

  <!-- Connection limits -->
  <max type="IntegerField"/>
  <max-src-states type="IntegerField"/>
  <max-src-conn type="IntegerField"/>
  <max-src-nodes type="IntegerField"/>
  <max-src-conn-rate type="IntegerField"/>
  <max-src-conn-rates type="IntegerField"/>      <!-- Time window -->
  <overload type="ModelRelationField"/>           <!-- Overload table alias -->

  <!-- Adaptive timeouts -->
  <adaptivestart type="IntegerField"/>
  <adaptiveend type="IntegerField"/>

  <!-- QoS / priority -->
  <prio type="OptionField">0-7</prio>
  <set-prio type="OptionField"/>
  <set-prio-low type="OptionField"/>
  <shaper1 type="ModelRelationField"/>            <!-- Pipe/queue target -->
  <shaper2 type="ModelRelationField"/>

  <!-- TCP flags -->
  <tcpflags1 type="OptionField"><Multiple>Y</Multiple></tcpflags1>
  <tcpflags2 type="OptionField"><Multiple>Y</Multiple></tcpflags2>
  <tcpflags_any type="BooleanField"/>

  <!-- Tagging -->
  <tag type="TextField"/>
  <tagged type="TextField"/>

  <!-- Categories and scheduling -->
  <categories type="ModelRelationField"><Multiple>Y</Multiple></categories>
  <sched type=".\ScheduleField"/>

  <!-- Logging -->
  <log type="BooleanField">0</log>

  <!-- Misc -->
  <allowopts type="BooleanField">0</allowopts>
  <nosync type="BooleanField">0</nosync>          <!-- Exclude from pfsync -->
  <description type="DescriptionField"/>
</rule>
```

### 3.2 Rule Priority System

Rules are organized by priority groups (from `FilterRuleContainerField`):

| Priority | Group | Value |
|----------|-------|-------|
| 1 | Floating rules | `200000` |
| 2 | Interface group rules | `300000 + group_sequence` |
| 3 | Single interface rules | `400000` |

The final sort order is `"{priority_group}.{sequence_number}"` (e.g., `200000.000100`). Rules are always evaluated in this order: floating first, then groups, then per-interface.

### 3.3 Rule Types

OPNsense manages four distinct rule types in a single `Filter` model:

1. **Filter Rules** (`rules.rule`) - Standard pass/block/reject rules
2. **Source NAT** (`snatrules.rule`) - Outbound NAT with target address/port
3. **NPTv6** (`npt.rule`) - IPv6 Network Prefix Translation
4. **1:1 NAT** (`onetoone.rule`) - Static BINAT/NAT mappings

### 3.4 Rule Validation

The `Filter::performValidation()` method enforces complex cross-field rules:

- Ports only valid for TCP/UDP protocols
- IP protocol version must match address family (IPv4 vs IPv6)
- "Any" cannot be combined with other aliases in multi-value fields
- ICMP type only valid for ICMP protocol
- Inverting sources/destinations only allowed for single targets
- TCP-specific options (statetimeout, max-src-conn, tcpflags) only for TCP
- Adaptive timeouts must be disabled together
- Gateway and reply-to are mutually exclusive
- Divert-to only valid for pass rules
- Shaper pipes and queues cannot be combined

### 3.5 Rule Serialization

The `FilterRuleContainerField::serialize()` method maps internal field names to pf-style format:

```
source_net         -> from
source_not         -> from_not
source_port        -> from_port
destination_net    -> to
destination_not    -> to_not
destination_port   -> to_port
enabled            -> disabled (inverted)
description        -> descr
action             -> type
replyto            -> reply-to
```

## 4. Alias System

### 4.1 Alias Types

OPNsense supports 14 alias types (from `Alias.xml`):

| Type | Description |
|------|-------------|
| `host` | Individual IP addresses |
| `network` | CIDR subnets |
| `port` | Port numbers/names |
| `url` | URL returning IP list |
| `urltable` | URL table with auto-refresh |
| `urljson` | JSON-format URL table |
| `geoip` | GeoIP country codes |
| `networkgroup` | Group of network aliases |
| `mac` | MAC addresses |
| `asn` | BGP ASN numbers |
| `dynipv6host` | Dynamic IPv6 host (interface-tracked) |
| `authgroup` | OpenVPN user groups |
| `internal` | System-managed (automatic) |
| `external` | Externally managed (advanced) |

### 4.2 Alias Model Structure

```xml
<alias type=".\AliasField">
  <enabled type="BooleanField">1</enabled>
  <name type=".\AliasNameField">
    <Required>Y</Required>
    <Constraints>
      <check001>
        <ValidationMessage>An alias with this name already exists.</ValidationMessage>
        <type>UniqueConstraint</type>
      </check001>
    </Constraints>
  </name>
  <type type="OptionField">host|network|port|url|...</type>
  <content type=".\AliasContentField"/>         <!-- Newline-separated entries -->
  <proto type="OptionField"><Multiple>Y</Multiple></proto>
  <interface type="InterfaceField"/>             <!-- For dynipv6host -->
  <counters type="BooleanField"/>                <!-- Enable hit counters -->
  <updatefreq type="NumericField"/>              <!-- URL refresh interval -->

  <!-- Authentication for URL-based aliases -->
  <username type="TextField"/>
  <password type="TextField"/>
  <authtype type="OptionField">Basic|Bearer|Header</authtype>

  <!-- Expiry -->
  <expire type="IntegerField">60-999999999</expire>

  <!-- Runtime statistics (volatile, not persisted) -->
  <current_items type="IntegerField" volatile="true"/>
  <last_updated type="TextField" volatile="true"/>
  <in_block_p type="IntegerField" volatile="true"/>
  <in_block_b type="IntegerField" volatile="true"/>
  <in_pass_p type="IntegerField" volatile="true"/>
  <in_pass_b type="IntegerField" volatile="true"/>
  <out_block_p type="IntegerField" volatile="true"/>
  <out_block_b type="IntegerField" volatile="true"/>
  <out_pass_p type="IntegerField" volatile="true"/>
  <out_pass_b type="IntegerField" volatile="true"/>

  <description type="DescriptionField"/>
</alias>
```

### 4.3 Alias Usage Tracking

The `Alias::whereUsed()` method scans the entire configuration to find where an alias is referenced. It searches 31 different config paths including:

- Legacy filter rules (`filter.rule.source/destination.address`)
- Legacy NAT rules (`nat.rule.source/destination.address`)
- New MVC filter rules (`OPNsense.Firewall.Filter.rules.rule.source_net`)
- Other aliases (nested alias references)

This enables the system to prevent deletion of aliases that are actively in use.

### 4.4 Alias Refactoring

When an alias is renamed, `Alias::refactor()` updates all references throughout the entire configuration, including:
- All filter rules referencing the old name
- All NAT rules referencing the old name
- Nested aliases that contain the old name in their content

### 4.5 Alias API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `searchItemAction()` | `GET /api/firewall/alias/searchItem` | List aliases with filtering by type/category |
| `addItemAction()` | `POST /api/firewall/alias/addItem` | Create new alias |
| `getItemAction($uuid)` | `GET /api/firewall/alias/getItem/$uuid` | Get alias details |
| `setItemAction($uuid)` | `POST /api/firewall/alias/setItem/$uuid` | Update alias (with rename refactoring) |
| `delItemAction($uuids)` | `POST /api/firewall/alias/delItem/$uuids` | Delete aliases (with usage check) |
| `toggleItemAction($uuid)` | `POST /api/firewall/alias/toggleItem/$uuid` | Enable/disable alias |
| `exportAction()` | `GET /api/firewall/alias/export` | Export aliases as JSON |
| `importAction()` | `POST /api/firewall/alias/import` | Import aliases from JSON |
| `reconfigureAction()` | `POST /api/firewall/alias/reconfigure` | Apply alias changes |
| `getTableSizeAction()` | `GET /api/firewall/alias/getTableSize` | Get pf table sizes |
| `getGeoIPAction()` | `GET /api/firewall/alias/getGeoIP` | Get GeoIP configuration |
| `updateAction($action)` | `POST /api/firewall/alias/update/$action` | Update GeoIP/bogons |
| `listCountriesAction()` | `GET /api/firewall/alias/listCountries` | List countries for GeoIP |
| `listNetworkAliasesAction()` | `GET /api/firewall/alias/listNetworkAliases` | List non-port aliases |
| `listCategoriesAction()` | `GET /api/firewall/alias/listCategories` | List categories with counts |

## 5. VPN Integration

### 5.1 WireGuard

The WireGuard module uses three models:

**Server** (`Wireguard/Server.xml`):
- `name`, `instance` (auto-numbered), `pubkey`, `privkey`
- `port`, `mtu`, `dns`, `tunneladdress`
- `peers` (ModelRelationField referencing Client model)
- `carp_depend_on` (VirtualIPField for HA)
- `debug`, `endpoint`, `peer_dns` (volatile)

**Client** (`Wireguard/Client.xml`):
- `name`, `pubkey`, `psk` (pre-shared key)
- `tunneladdress`, `serveraddress`, `serverport`
- `endpoint` (volatile), `keepalive`
- `servers` (ModelRelationField, volatile - reverse lookup)

**General** (`Wireguard/General.xml`):
- Simple `enabled` toggle

Backend actions (`actions_wireguard.conf`):
- `start`, `stop`, `restart` - Service lifecycle
- `configure` - Generate WireGuard config
- `gen_keypair`, `gen_psk` - Key generation
- `show`, `showconf`, `showhandshake` - Status/diagnostics
- `renew` - DNS renewal for stale connections

### 5.2 OpenVPN

The OpenVPN model (`OpenVPN/OpenVPN.xml`) is the most complex VPN model:

**Instances** (`InstanceField`):
- `role`: server or client
- `dev_type`: TUN, TAP, or DCO (OpenVPN Data Channel Offload)
- `proto`: UDP/TCP with IPv4/IPv6 variants
- `server`/`server_ipv6`: Tunnel network with CIDR
- `cert`, `ca`, `crl`: Certificate management
- `authmode`: Multiple authentication servers
- `data-ciphers`: Recommended + Legacy cipher options
- `data-ciphers-fallback`: Fallback cipher selection
- `tls_key`: Static key reference
- `various_flags`, `various_push_flags`: OpenVPN flags
- `carp_depend_on`: HA dependency
- DNS, NTP, WINS push options

**Overwrites** (`OpenVPNServerField`):
- Per-client configuration overrides by common_name
- Route/network overrides
- DNS/NTP/WINS overrides

**StaticKeys**:
- TLS static key management
- Mode selection (auth, crypt, crypt-v2)

### 5.3 OpenVPN Export

The `Export.xml` model handles OpenVPN client export configuration with settings for:
- Proxy configuration
- Register DNS
- IPv6 support
- Various export formats

## 6. Traffic Shaper (QoS)

### 6.1 Architecture

OPNsense's traffic shaper uses FreeBSD's `dn` (dummynet) framework with three resource types:

**Pipes** (bandwidth limiters):
```xml
<pipe>
  <number>1-65535</number>           <!-- Pipe ID -->
  <bandwidth>1+</bandwidth>          <!-- Bandwidth value -->
  <bandwidthMetric>bit|Kbit|Mbit|Gbit</bandwidthMetric>
  <queue>2-100</queue>               <!-- Queue size -->
  <mask>none|src-ip|dst-ip|src-ip6|dst-ip6</mask>
  <buckets>1-65535</buckets>
  <scheduler>fifo|rr|qfq|fq_codel|fq_pie</scheduler>
  <delay>1-3000</delay>              <!-- Latency in ms -->

  <!-- CoDel active queue management -->
  <codel_enable>0</codel_enable>
  <codel_target>1-10000</codel_target>
  <codel_interval>1-10000</codel_interval>
  <codel_ecn_enable>0</codel_ecn_enable>

  <!-- PIE active queue management -->
  <pie_enable>0</pie_enable>

  <!-- FQ-CoDel parameters -->
  <fqcodel_quantum>1-65535</fqcodel_quantum>
  <fqcodel_limit>1-65535</fqcodel_limit>
  <fqcodel_flows>1-65535</fqcodel_flows>
</pipe>
```

**Queues** (weighted sub-queues):
```xml
<queue>
  <number>1-65535</number>
  <pipe>ModelRelationField -> pipe</pipe>    <!-- Parent pipe -->
  <weight>1-100</weight>                     <!-- WFQ weight -->
  <mask>none|src-ip|dst-ip|...</mask>
  <buckets>1-65535</buckets>
  <codel_enable>0</codel_enable>
  <pie_enable>0</pie_enable>
</queue>
```

**Rules** (traffic classifiers):
```xml
<rule>
  <enabled>1</enabled>
  <sequence>1-1000000</sequence>
  <interface>InterfaceField</interface>
  <proto>ip|tcp|udp|icmp|...</proto>
  <source>NetworkField (AsList)</source>
  <source_not>BooleanField</source_not>
  <src_port>PortField</src_port>
  <destination>NetworkField (AsList)</destination>
  <destination_not>BooleanField</destination_not>
  <dst_port>PortField</dst_port>
  <dscp><Multiple>Y</Multiple>              <!-- DSCP marking -->
    be|ef|af11-af43|cs1-cs7
  </dscp>
  <direction>in|out</direction>
  <target>ModelRelationField -> pipe|queue</target>
  <iplen>2-65535</iplen>                    <!-- Packet size filter -->
</rule>
```

### 6.2 Scheduler Options

| Scheduler | Description |
|-----------|-------------|
| `fifo` | First-In-First-Out |
| `rr` | Deficit Round Robin |
| `qfq` | Quick Fair Queueing |
| `fq_codel` | FlowQueue-CoDel (default recommended) |
| `fq_pie` | FlowQueue-PIE |

### 6.3 Integration with Firewall Rules

Traffic shaper pipes/queues can be referenced from firewall rules via `shaper1` and `shaper2` fields, allowing per-rule QoS. The model enforces that pipes and queues cannot be mixed in the same rule pair.

## 7. IDS/IPS (Suricata Integration)

### 7.1 Model Structure

The IDS model (`IDS/IDS.xml`) manages Suricata configuration:

**General Settings**:
- `enabled`, `mode`: PCAP (IDS) / Netmap (IPS) / Divert (IPS)
- `interfaces`: Network interfaces to monitor
- `homenet`: Trusted networks (RFC1918 default)
- `defaultPacketSize`: 82-65535
- `MPMAlgo`: Aho-Corasick / Hyperscan pattern matcher
- `detect.Profile`: low / medium / high / custom
- `promisc`: Promiscuous mode
- `syslog`, `syslog_eve`: Logging options
- `LogPayload`, `verbosity`: Debug options

**EVE Log Configuration**:
- HTTP logging (enable, extended, dump headers)
- TLS logging (enable, extended, session resumption, custom fields: ja3/ja3s/ja4, certificate chain, etc.)

**Rules** (`PolicyRulesField`):
- `sid`: Suricata rule ID
- `enabled`, `action`: alert / drop
- `msg`, `source`: Volatile metadata

**Policies**:
- `prio`: Evaluation priority
- `action`: disable / alert / drop (multi-select)
- `rulesets`: ModelRelationField to files
- `content`: PolicyContentField (multi-select rule patterns)
- `new_action`: default / alert / drop / disable

**User-Defined Rules**:
- `source`, `destination`: NetworkField
- `fingerprint`: SSL fingerprint (59-char hex)
- `action`: alert / drop / pass
- `bypass`: Skip IDS inspection

**Files** (rulesets):
- `filename`: Ruleset name
- `enabled`: Toggle

### 7.2 API Design

The IDS SettingsController provides comprehensive rule management:

| Endpoint | Description |
|----------|-------------|
| `searchInstalledRulesAction()` | Paginated rule search with backend query |
| `getRuleInfoAction($sid)` | Single rule details with CVE/URL links |
| `listRulesetsAction()` | Available rulesets with metadata |
| `setRulesetAction($filename)` | Update ruleset settings |
| `toggleRuleAction($sids)` | Enable/disable/alert/drop rules |
| `toggleRulesetAction($filenames)` | Toggle entire rulesets |
| `searchUserRuleAction()` | Custom rules CRUD |
| `searchPolicyRuleAction()` | Policy rule overrides |

### 7.3 Backend Integration

IDS operations delegate to configd:
- `ids query rules` - Database query for installed rules
- `ids list installablerulesets` - Available rulesets
- `ids list rulemetadata` - Rule metadata categories
- Template generation via Jinja2 for `suricata.yaml`

## 8. API Design Patterns

### 8.1 REST API Structure

OPNsense uses a consistent URL pattern:

```
/api/{module}/{controller}/{action}/{parameters}
```

Examples:
```
GET  /api/firewall/filter/searchRule?interface=lan
GET  /api/firewall/filter/getRule/{uuid}
POST /api/firewall/filter/addRule
POST /api/firewall/filter/setRule/{uuid}
POST /api/firewall/filter/delRule/{uuid}
POST /api/firewall/filter/toggleRule/{uuid}
```

### 8.2 Base Controller Methods

The `ApiMutableModelControllerBase` provides these inherited methods:

| Method | Description |
|--------|-------------|
| `searchBase($modelPath, $fields, $sortField)` | Paginated search with filtering |
| `getBase($name, $modelPath, $uuid)` | Get single item |
| `addBase($name, $modelPath)` | Create new item |
| `setBase($name, $modelPath, $uuid)` | Update existing item |
| `delBase($modelPath, $uuids)` | Delete items (comma-separated UUIDs) |
| `toggleBase($modelPath, $uuid, $enabled)` | Toggle enabled state |
| `searchRecordsetBase($records, ...)` | Search pre-loaded records |
| `downloadRulesBase(...)` | Export rules as JSON |
| `uploadRulesBase(...)` | Import rules from JSON |

### 8.3 Request/Response Format

**Search Request** (POST):
```json
{
  "current": 1,
  "rowCount": 10,
  "searchPhrase": "ssh",
  "sort": {"sequence": "asc"},
  "category": ["cat1", "cat2"],
  "interface": "lan"
}
```

**Search Response**:
```json
{
  "rows": [...],
  "rowCount": 10,
  "total": 42,
  "current": 1
}
```

**Item Response**:
```json
{
  "rule": {
    "enabled": "1",
    "sequence": "100",
    "action": "pass",
    "interface": "lan",
    "protocol": "tcp",
    "source_net": "any",
    "destination_net": "192.168.1.0/24",
    "destination_port": "443",
    "description": "Allow HTTPS"
  }
}
```

### 8.4 Input Sanitization

Controllers use `SanitizeFilter` for search phrase sanitization, and the model layer enforces field-level validation through the XML-defined constraints.

### 8.5 Config Locking

Before modifying configuration, controllers call `Config::getInstance()->lock()` to acquire an exclusive lock, preventing concurrent modifications. This is released on save or unlock.

## 9. Plugin System

### 9.1 Plugin Repository Structure

Plugins are maintained in a separate repository (`opnsense/plugins`) with 90+ plugins organized by category:

```
plugins/
  net/              # Networking (chrony, freeradius, frr, haproxy, zerotier...)
  security/         # Security (acme-client, crowdsec, openvpn, tailscale, tor...)
  dns/              # DNS (bind, ddclient, dnscrypt-proxy)
  mail/             # Mail (postfix, rspamd)
  www/              # Web (caddy, nginx, squid)
  sysutils/         # System (nut, smart, vmware, xen)
  net-mgmt/         # Management (telegraf, zabbix-agent, collectd)
  devel/            # Development (debug, grid_example, helloworld)
  misc/             # Themes (cicada, tukan, vicuna)
  databases/        # Databases (redis)
  benchmarks/       # Benchmarks (iperf)
```

### 9.2 Plugin Makefile

Each plugin has a minimal Makefile:

```makefile
PLUGIN_NAME=            frr
PLUGIN_VERSION=         1.52
PLUGIN_REVISION=        1
PLUGIN_COMMENT=         "The FRRouting Protocol Suite"
PLUGIN_DEPENDS=         frr10-pythontools
PLUGIN_MAINTAINER=      ad@opnsense.org
PLUGIN_TIER=            2

.include "../../Mk/plugins.mk"
```

The `plugins.mk` include provides all build logic. The `PLUGIN_TIER` indicates support level (1=core, 2=community, 3=experimental).

### 9.3 Plugin Internal Structure

A plugin follows the same MVC structure as core modules:

```
net/frr/src/
  opnsense/
    mvc/app/
      controllers/OPNsense/Quagga/    # API + UI controllers
      models/OPNsense/Quagga/         # XML + PHP model pairs
      views/OPNsense/Quagga/          # Volt templates
    scripts/frr/                      # Backend helper scripts
    service/                          # configd action definitions
  etc/                                # System configuration files
```

### 9.4 Plugin Configuration Integration

Plugins register with the core system through:

1. **ACL** (`ACL/ACL.xml`) - Page access permissions:
   ```xml
   <acl>
     <page-filter-api>
       <name>Firewall: Rules</name>
       <patterns>
         <pattern>ui/firewall/filter/*</pattern>
         <pattern>api/firewall/filter/*</pattern>
       </patterns>
     </page-filter-api>
   </acl>
   ```

2. **Menu** (`Menu/Menu.xml`) - Navigation entries:
   ```xml
   <Menu>
     <Firewall>
       <Firewall cssclass="fa fa-check fa-fw">
         <Filter>
           <url>/ui/firewall/filter/</url>
         </Filter>
         <NAT>
           <SourceNat order="80">
             <url>/ui/firewall/source_nat/</url>
           </SourceNat>
         </NAT>
       </Firewall>
     </Firewall>
   </Menu>
   ```

3. **configd Actions** (`service/conf/actions.d/`) - Backend operations:
   ```ini
   [configure]
   command:/usr/local/opnsense/scripts/wireguard/wg-service-control.php
   parameters:-a configure %s
   type:script
   message:configure wireguard instances (%s)
   ```

4. **Migrations** (`Migrations/`) - Version upgrade scripts

### 9.5 Plugin API

Plugins can:
- Define new MVC models with XML schemas
- Register API endpoints via controllers
- Add configd backend actions
- Define menu entries and ACL permissions
- Add dashboard widgets
- Hook into syshooks for lifecycle events
- Define custom field types in their `FieldTypes/` directory

## 10. Key Design Patterns for VectorOS

### 10.1 XML-Defined Configuration Schema

**Pattern**: Declarative XML schemas define configuration structure, field types, constraints, and relationships. PHP/Rust code provides validation logic and API endpoints.

**VectorOS Application**: Use a similar declarative approach but with Rust-native format (TOML, YAML, or a custom schema DSL). Define configuration schemas once and generate:
- Serialization/deserialization code
- Validation logic
- API endpoint handlers
- Frontend form generation

### 10.2 Two-Tier Frontend/Backend Architecture

**Pattern**: Frontend (UI/API) communicates with backend (service management) via IPC. Backend handles config generation and service lifecycle.

**VectorOS Application**: Already follows this pattern with the Rust control plane + VPP data plane. Extend it:
- Control plane (Rust/Axum) = frontend tier
- VPP plugin execution = backend tier
- Communication via VPP binary API (already implemented)

### 10.3 Model-Driven API Generation

**Pattern**: Controllers inherit from base class providing standard CRUD operations (search, get, add, set, delete, toggle). Custom logic only for special operations.

**VectorOS Application**: Create a generic model trait in Rust:
```rust
trait ModelController {
    type Model;
    fn search(&self, query: SearchQuery) -> SearchResult;
    fn get(&self, uuid: &str) -> Option<Self::Model>;
    fn create(&self, data: Self::Model) -> Result<String>;
    fn update(&self, uuid: &str, data: Self::Model) -> Result<()>;
    fn delete(&self, uuids: &[str]) -> Result<()>;
    fn toggle(&self, uuid: &str, enabled: bool) -> Result<()>;
}
```

### 10.4 Alias-Based Abstraction

**Pattern**: Aliases allow rules to reference named groups of addresses/ports/networks. Rules use aliases instead of raw values. Aliases are resolved at rule application time.

**VectorOS Application**: Implement aliases as first-class entities:
```rust
struct Alias {
    uuid: String,
    name: String,
    alias_type: AliasType,  // Host, Network, Port, URL, GeoIP
    content: Vec<String>,
    enabled: bool,
}

// Rules reference aliases by name
struct FirewallRule {
    source: NetworkTarget,      // Can be IP, CIDR, or alias name
    destination: NetworkTarget,
    // ...
}

enum NetworkTarget {
    Address(IpAddr),
    Network(Cidr),
    Alias(String),           // Reference by name
    Any,
}
```

### 10.5 Priority-Based Rule Ordering

**Pattern**: Rules organized by priority groups (floating > groups > interface), with sequence numbers within each group. Sort key is `"{priority}.{sequence}"`.

**VectorOS Application**: Implement similar rule ordering for VPP ACL rules:
- Priority group: determines evaluation order
- Sequence: fine-grained ordering within group
- UUID-based identification for stable references

### 10.6 Configd-Style Backend Actions

**Pattern**: Backend actions defined in config files with command, parameters, type, and message. Frontend invokes actions by name.

**VectorOS Application**: Define VPP operations as named actions:
```toml
[actions.filter.reload]
command = "vppctl"
parameters = "acl-plugin flush"
type = "script"
message = "Reloading firewall rules"

[actions.wireguard.start]
command = "wg-quick"
parameters = "up %s"
type = "script"
message = "Starting WireGuard interface %s"
```

### 10.7 Plugin Architecture

**Pattern**: Plugins follow convention-over-configuration. Minimal Makefile declares metadata. Shared build system handles packaging. Plugins register via ACL, Menu, configd actions, and MVC models.

**VectorOS Application**: Plugin system for VPP plugins:
```
plugins/
  pppoe-client/
    Makefile                    # Plugin metadata
    src/
      vpp-plugins/              # C plugin code
      control-plane/            # Rust API handlers
      models/                   # Configuration schema
    service/
      actions.conf              # Backend operations
```

### 10.8 Configuration Rollback

**Pattern**: Models support rollback to previous versions by reading backup config files and replacing the relevant XML section.

**VectorOS Application**: Implement config snapshot and rollback:
```rust
impl Config {
    fn snapshot(&self) -> ConfigSnapshot;
    fn rollback(&mut self, snapshot: &ConfigSnapshot) -> Result<()>;
    fn list_snapshots(&self) -> Vec<ConfigSnapshot>;
}
```

### 10.9 Where-Used Analysis

**Pattern**: Before deleting any configuration entity, scan all references across the entire config tree. Present user with a list of dependent configurations.

**VectorOS Application**: Implement dependency tracking for:
- Aliases referenced by firewall rules
- Interfaces referenced by routes, NAT, VPN
- Certificates referenced by VPN, HTTPS, RADIUS

## 11. Comparison with VectorOS Current State

### 11.1 What VectorOS Already Has

| Feature | OPNsense | VectorOS |
|---------|----------|----------|
| Data plane | FreeBSD pf | VPP (higher performance) |
| Control plane | PHP + Python | Rust (type safety, performance) |
| API | PHP MVC (Phalcon) | Axum (async, type-safe) |
| Frontend | jQuery + Bootstrap | Svelte + Tailwind (modern) |
| VPN | WireGuard + OpenVPN | Planned |
| Config storage | XML | TOML |

### 11.2 What VectorOS Should Adopt from OPNsense

1. **Declarative model definitions** - XML-like schema for configuration (could use Rust derive macros or TOML schema)
2. **Alias system** - Essential for firewall usability
3. **Rule priority groups** - Floating, group, interface ordering
4. **configd-style backend** - Separate service management from API
5. **Plugin convention** - Standardized plugin structure
6. **Config rollback** - Safe configuration changes
7. **Where-used analysis** - Prevent deletion of referenced entities
8. **Migration system** - Version-based config upgrades
9. **Category system** - Organize rules/aliases by category
10. **EVE logging integration** - Structured logging for IDS/IPS

### 11.3 Where VectorOS Can Improve

1. **No XML overhead** - Use Rust-native serialization (serde)
2. **Async by default** - Non-blocking I/O throughout
3. **Type safety** - Compile-time guarantees vs runtime validation
4. **Performance** - VPP's DPDK-based data plane vs FreeBSD kernel pf
5. **Modern frontend** - Svelte vs jQuery/Bootstrap
6. **API-first design** - RESTful from the start, not bolted on

## 12. Implementation Recommendations

### 12.1 Configuration Schema Definition

Use Rust derive macros for declarative model definitions:

```rust
#[derive(Model)]
#[model(mount = "firewall/filter", version = "1.0.0")]
pub struct FilterModel {
    pub rules: Vec<FilterRule>,
    pub snat_rules: Vec<SnatRule>,
    pub npt_rules: Vec<NptRule>,
    pub onetoone_rules: Vec<OneToOneRule>,
}

#[derive(ModelField)]
pub struct FilterRule {
    #[field(required, default = true)]
    pub enabled: bool,
    #[field(range = 1..999999)]
    pub sequence: u32,
    #[field(required, default = "pass")]
    pub action: Action,
    #[field(multiple)]
    pub interface: Vec<String>,
    pub source: NetworkTarget,
    pub destination: NetworkTarget,
    // ...
}
```

### 12.2 API Generation

Auto-generate REST endpoints from model definitions:

```rust
// Generated from FilterModel
GET  /api/firewall/filter/search?interface=lan
GET  /api/firewall/filter/get/:uuid
POST /api/firewall/filter/add
POST /api/firewall/filter/set/:uuid
POST /api/firewall/filter/delete/:uuids
POST /api/firewall/filter/toggle/:uuid
POST /api/firewall/filter/move/:selected/:target
```

### 12.3 Alias Integration

```rust
impl FilterRule {
    pub fn resolve_source(&self, aliases: &AliasStore) -> Vec<IpNetwork> {
        match &self.source {
            NetworkTarget::Alias(name) => aliases.resolve(name),
            NetworkTarget::Address(addr) => vec![(*addr).into()],
            NetworkTarget::Network(net) => vec![*net],
            NetworkTarget::Any => vec![],  // Special handling
        }
    }
}
```

This comprehensive analysis of OPNsense provides a roadmap for VectorOS to adopt proven firewall management patterns while leveraging Rust's advantages in performance and type safety.
