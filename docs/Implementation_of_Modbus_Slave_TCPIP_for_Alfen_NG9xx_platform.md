# [cite\_start]Modbus Slave TCP/IP [cite: 1]

[cite\_start]**Implementation of Modbus Slave TCP/IP for Alfen NG9xx platform** [cite: 2]

| | |
| :--- | :--- |
| **Author:** | [cite\_start]T. Nederlof [cite: 2] |
| **Version:** | [cite\_start]2.3 [cite: 2] |
| **Date:** | [cite\_start]30-10-2020 [cite: 2] |

-----

## [cite\_start]Table of contents [cite: 7]

| Section | Title | [cite\_start]Page [cite: 8] |
| :--- | :--- | :--- |
| 1 | Introduction | [cite\_start]3 [cite: 9] |
| 1.1 | Configuration options | [cite\_start]4 [cite: 9] |
| 1.2 | Reading registers | [cite\_start]4 [cite: 9] |
| 1.3 | Writing registers | [cite\_start]5 [cite: 9] |
| 1.3.1 | Maximum current | [cite\_start]5 [cite: 9] |
| 1.4 | Phase rotation | [cite\_start]5 [cite: 9] |
| 2 | Enabling Modbus Slave in charging stations | [cite\_start]6 [cite: 9] |
| 2.1 | Activate Active Load Balancing | [cite\_start]6 [cite: 9] |
| 2.2 | Energy Management Systems | [cite\_start]8 [cite: 9] |
| 3 | Modbus Register table | [cite\_start]10 [cite: 9] |
| 3.1 | Product identification registers | [cite\_start]10 [cite: 9] |
| 3.2 | Station status registers | [cite\_start]11 [cite: 9] |
| 3.3 | SCN registers | [cite\_start]12 [cite: 9] |
| 3.4 | Socket measurement registers | [cite\_start]13 [cite: 9] |
| 3.4.1 | Mode 3 state listing | [cite\_start]15 [cite: 9] |

-----

## [cite\_start]1. Introduction [cite: 14]

[cite\_start]This document is a draft concerning the implementation of Modbus slave functionality for Alfen N.V.'s NG9xx charging stations. [cite: 15] [cite\_start]All information herein may be subject to future changes and should be considered an indication of supported functionality. [cite: 15]

[cite\_start]There is a significant distinction between the master and slave roles within the Modbus communication protocol. [cite: 16] [cite\_start]This implementation details the slave role, which serves a Modbus master. [cite: 17] [cite\_start]These roles are also known as server and client, respectively, when using Modbus over TCP/IP. [cite: 17] [cite\_start]The master is responsible for initiating the connection to the slave and sending requests to either read or write to specific Modbus registers. [cite: 18]

The current Modbus implementation can support up to two Modbus TCP/IP masters connected simultaneously; [cite\_start]UDP is not supported. [cite: 19] [cite\_start]A keep-alive timeout of 60 seconds is in place; if no new read or write is received within this period, the connection to the Modbus master will be closed. [cite: 20] [cite\_start]The Modbus master must connect to the IP address of the Modbus slave's wired Ethernet on port 502. [cite: 21] [cite\_start]The system accepts requests with specific slave addresses: charging station-related registers use slave address 200, while socket-related registers use slave address 1 or 2, depending on the socket. [cite: 21] [cite\_start]All communication must adhere to the big-endian format. [cite: 22] [cite\_start]Any new values written to registers by Modbus masters are logged in the charging station, provided the new value differs from the current one. [cite: 23]

[cite\_start]This document is based on: [cite: 24]

  * [cite\_start]Firmware version 4.10 [cite: 25]
  * [cite\_start]Service Installer Application 3.4.10-130 [cite: 26]

### [cite\_start]1.1 Configuration options [cite: 31]

[cite\_start]The Modbus slave functionality can be configured using the Alfen ACE Service Installer (version 4.0 or higher) with a service account or an administrator account. [cite: 32] [cite\_start]To enable the Modbus slave functionality, the charging station must have a license key for "Active load balancing". [cite: 33] [cite\_start]The configured settings detailed below are persistent and will be preserved if the charging station reboots. [cite: 34]

| Name | Function |
| :--- | :--- |
| **Allow reading** | Allows the reading of Modbus registers via TCP/IP. [cite\_start]This is turned off by default. [cite: 35] |
| **Allow writing maximum currents** | Permits the writing of maximum current Modbus registers. [cite\_start]This is turned off by default. [cite: 35] |
| **Enable sockets** | [cite\_start]Enables the charging station to use the written maximum current values for sockets when calculating the actual maximum current for all sockets. [cite: 35] |
| **Enable SCN** | [cite\_start]Enables the charging station to use the written maximum current values for SCN when calculating the actual maximum current for all SCN phases. [cite: 35] |
| **Validity time** | [cite\_start]This is the time in seconds that the charging station waits for an updated maximum current from a Modbus master before it reverts to the safe current. [cite: 35] [cite\_start]The validity time is the same for all maximum currents, but each has its own remaining valid time that is updated whenever that specific maximum current is set via Modbus. [cite: 35] [cite\_start]The default is 60 seconds. [cite: 35] [cite\_start]It's recommended that the Modbus master's polling time is less than this validity time. [cite: 35] |
| **IP Address allocation** | [cite\_start]DHCP or fixed IP. [cite: 35] |
| **Port** | [cite\_start]502 [cite: 35] |
| **Modbus slave addresses** | [cite\_start]1: measurements socket 1\<br\>2: measurements socket 2 (if available) [cite: 35] |
| **Supported Modbus functions** | [cite\_start]`$0\times03;$: Read Holding Registers<br>`$0\\times06:$ Write Single Register\<br\>\`$0\\times10$: Write Multiple Registers [cite: 35] |

### [cite\_start]1.2 Reading registers [cite: 36]

[cite\_start]The Modbus slave implementation supports reading holding registers using Modbus function code 3. [cite: 37] [cite\_start]It is possible to request multiple contiguous registers in a single Modbus request. [cite: 37] [cite\_start]If a register is reserved or unavailable, the reply for that register will be filled with Not a Number (NaN), represented as `0xFFFF` for a 16-bit register. [cite: 38] [cite\_start]Some Modbus registers contain string data types. [cite: 39] [cite\_start]In these string registers, each 16-bit Modbus register holds two 8-bit ASCII characters. [cite: 40] [cite\_start]A string is always terminated with a trailing zero. [cite: 41] [cite\_start]Note that reading registers is done in network byte order. [cite: 42]

### [cite\_start]1.3 Writing registers [cite: 48]

[cite\_start]The Modbus slave implementation allows for writing to holding registers. [cite: 49] [cite\_start]When writing a value with a data type that spans multiple Modbus registers, all registers for that value must be written in a single write request. [cite: 49] [cite\_start]For instance, a 32-bit float requires both consecutive 16-bit registers to be written in one request. [cite: 50] [cite\_start]If a write request does not include all necessary registers, it will be denied, and a Modbus error will be returned. [cite: 51] [cite\_start]Note that writing registers is done in network byte order. [cite: 52]

#### [cite\_start]1.3.1 Maximum current [cite: 53]

[cite\_start]It's possible to set the maximum current for a specific socket or a specific phase of the SCN network via Modbus. [cite: 54] [cite\_start]Each maximum current setting has associated read-only registers for the enabled property, the actual maximum current, the configured safe current, and the remaining validity time. [cite: 55] [cite\_start]When the maximum current is written via Modbus, the remaining valid time is reset to the configured validity time. [cite: 56] [cite\_start]For example, with a validity time of 60 seconds, if the maximum current was last written 10 seconds ago, the remaining valid time register will read 50. [cite: 57]

[cite\_start]The maximum current and its remaining valid time are not saved during a reboot of the charging station. [cite: 58] [cite\_start]Since the 'enabled' and 'safe current' settings are persistent, the charging station will first fall back to the safe current and wait for the Modbus master to resend the maximum current. [cite: 59] [cite\_start]If a maximum current is enabled but not updated by the Modbus master within the specified time, the station defaults to its safe current. [cite: 60] [cite\_start]The safe current must be configured before the maximum current can be set through Modbus. [cite: 61] [cite\_start]This value can also be configured by the back office. [cite: 62] [cite\_start]The internal processing of a newly written maximum current can take some time. [cite: 63] [cite\_start]The time it takes for the connected car to adapt to this new current depends on several factors, including the car's own response speed. [cite: 64]

### [cite\_start]1.4 Phase rotation [cite: 65]

[cite\_start]The naming of phases is relative to the installation and the incoming phase rotation at the charging station. [cite: 66] [cite\_start]This can lead to confusion on the Modbus master's side, particularly when multiple charging stations are connected. [cite: 67] [cite\_start]For example, one charging station might be connected in the L1-L2-L3 phase order, while a second is wired L2-L3-L1. [cite: 68] [cite\_start]In this scenario, if the Modbus master needs to read the current for phase L1, it would read the L1 phase register on the first station but the L2 phase register on the second station. [cite: 69]

-----

## [cite\_start]2. Enabling Modbus Slave in charging stations [cite: 74]

[cite\_start]Modbus Slave over TCP/IP is activated when the station is set up to communicate with an Energy Management System (EMS) and when Active Load Balancing is enabled. [cite: 75] [cite\_start]Active Load Balancing is the feature that currently uses Modbus Slave TCP/IP for communication. [cite: 76]

[cite\_start]**Note:** Active Load Balancing is a locked feature and requires purchase to unlock. [cite: 77, 78] [cite\_start]After purchase, the station's unique license key is updated. [cite: 78]

[cite\_start]This section outlines the steps to enable Modbus Slave TCP/IP. [cite: 79] [cite\_start]A Service Installer Application (SIA) account is necessary to configure the charging station on-site. [cite: 80] [cite\_start]Accounts can be requested at [https://support.alfen.com](https://support.alfen.com). [cite: 81]

### [cite\_start]2.1 Activate Active Load Balancing [cite: 82]

[cite\_start]Active Load Balancing can be enabled in the 'Load Balancing' section of the Service Installer Application. [cite: 83] [cite\_start]Select "Active balancing" from the left-hand menu. [cite: 84]

*[Image: A screenshot of the Alfen Service Installer Application, showing the 'Load balancing' section. [cite\_start]'Active balancing' is selected, and the "Active Load Balancing" checkbox is ticked.]* [cite: 72]

[cite\_start]**Note:** The 'Allow 1- and 3-phased charging' checkbox must be checked to permit an Energy Management System to control the switching between single-phase and three-phase charging. [cite: 103] [cite\_start]This option must be enabled locally using the Service Installer Application. [cite: 104]

[cite\_start]In the Active Balancing menu, you must select a Data Source: [cite: 122]

  * [cite\_start]**Meter:** This configures the charging station in the 'Master' role. [cite: 123] [cite\_start]As a 'Master', the station calculates the available power for charging vehicles, prioritizing other consumers. [cite: 125, 126]
  * [cite\_start]**Energy Management System:** This configures the charging station in the 'Slave' role. [cite: 124] [cite\_start]As a 'Slave', the station follows commands from an external device like an EMS, which determines charging priority. [cite: 127, 128]

*[Image: A screenshot of the 'Active balancing' menu, showing the 'Data Source' dropdown with 'Energy Management System' selected.]*

### [cite\_start]2.2 Energy Management Systems [cite: 133]

[cite\_start]When 'EMS' is selected as the data source, the charging station is configured as a 'Slave'. [cite: 134] [cite\_start]A 'TCP/IP EMS' option will then appear in the left-hand menu. [cite: 135] [cite\_start]Modbus TCP/IP is the default and currently the only protocol that can be selected. [cite: 136]

[cite\_start]*[Image: A screenshot displaying the 'Modbus TCP/IP EMS' configuration options, including 'Mode' and 'Validity Time (s)'.]* [cite: 131]

The available options are:

  * [cite\_start]**Mode:** [cite: 150]
      * [cite\_start]**Socket:** Allows control of each individual socket. [cite: 151]
      * [cite\_start]**SCN:** Allows control over the entire charging station or a complete Smart Charging Network as a single entity. [cite: 152]
  * [cite\_start]**Validity time (s):** This is the period after which the station assumes the EMS is no longer available and reverts to the safe current setting from the 'Active balancing' menu. [cite: 153] [cite\_start]Register values must be rewritten before this time expires. [cite: 154]

[cite\_start]**Configuring the IP address:** [cite: 155]
[cite\_start]The charging station defaults to automatic IP allocation via DHCP, which can be used for Modbus Slave TCP/IP operation. [cite: 156, 157] [cite\_start]To find the station's identity, you can use a service like mDNS. [cite: 158]

  * [cite\_start]**Service type:** \_alfen.\_tcp.local [cite: 159]

[cite\_start]Alternatively, a fixed IP address can be used. [cite: 165]

1.  [cite\_start]Navigate to the 'Connectivity' tab. [cite: 166]
2.  [cite\_start]Click 'Wired' in the left-hand menu. [cite: 167]
3.  [cite\_start]Select 'Fixed IP address'. [cite: 168]
4.  [cite\_start]Fill in the required network details (IP address, Netmask, Gateway address, DNS). [cite: 169, 191, 193, 196, 198, 202]

[cite\_start]*[Image: A screenshot of the 'Connectivity' -\> 'Wired' settings screen, where a fixed IP address can be configured.]* [cite: 174]

-----

## [cite\_start]3. Modbus Register table [cite: 212]

### [cite\_start]3.1 Product identification registers [cite: 213]

[cite\_start]These registers are accessible using slave address 200. [cite: 214]

| Name | Start Address | End Address | Num of Registers | R/W | Data Type | Units | Additional Info |
| :--- | :--- | :--- | :--- | :-: | :--- | :--- | :--- |
| **Name** | 100 | 116 | 17 | R | STRING | n.a. | [cite\_start]"ALF\_1000" [cite: 209] |
| **Manufacturer** | 117 | 121 | 5 | R | STRING | n.a. | [cite\_start]"Alfen NV" [cite: 209] |
| **Modbus table version** | 122 | 122 | 1 | R | SIGNED16 | n.a. | [cite\_start]1 [cite: 209] |
| **Firmware version** | 123 | 139 | 17 | R | STRING | n.a. | [cite\_start]"3.4.0-2990" [cite: 209] |
| **Platform type** | 140 | 156 | 17 | R | STRING | n.a. | [cite\_start]"NG910" [cite: 209] |
| **Station serial number** | 157 | 167 | 11 | R | STRING | n.a. | [cite\_start]"00000R000" [cite: 209] |
| **Date year** | 168 | 168 | 1 | R | SIGNED16 | yr | [cite\_start]2019 [cite: 209] |
| **Date month** | 169 | 169 | 1 | R | SIGNED16 | mon | [cite\_start]03 [cite: 209] |
| **Date day** | 170 | 170 | 1 | R | SIGNED16 | d | [cite\_start]11 [cite: 209] |
| **Time hour** | 171 | 171 | 1 | R | SIGNED16 | hr | [cite\_start]12 [cite: 209] |
| **Time minute** | 172 | 172 | 1 | R | SIGNED16 | min | [cite\_start]01 [cite: 209] |
| **Time second** | 173 | 173 | 1 | R | SIGNED16 | s | [cite\_start]04 [cite: 209] |
| **Uptime** | 174 | 177 | 4 | R | UNSIGNED64 | 0.001s | [cite\_start]100 [cite: 209] |
| **Time zone** | 178 | 178 | 1 | R | SIGNED16 | 1 min | [cite\_start]Time zone offset to UTC in minutes. [cite: 209] |

### [cite\_start]3.2 Station status registers [cite: 221]

[cite\_start]These registers are accessible using slave address 200. [cite: 222]

| Description | Start Address | End Address | Num of Registers | R/W | Data Type | Step | Additional Info |
| :--- | :--- | :--- | :--- | :-: | :--- | :--- | :--- |
| **Station Active Max Current** | 1100 | 1101 | 2 | R | FLOAT32 | 1A | [cite\_start]The actual max current [cite: 218] |
| **Temperature** | 1102 | 1103 | 2 | R | FLOAT32 | $1^{\\circ}C$ | [cite\_start]Board temperature, not environment temperature. [cite: 218] |
| **OCPP state** | 1104 | 1104 | 1 | R | UNSIGNED16 | N.A. | [cite\_start]To verify back office connection. [cite: 218] |
| **Nr of sockets** | 1105 | 1105 | 1 | R | UNSIGNED16 | N.A. | [cite\_start]Number of sockets [cite: 218] |

### [cite\_start]3.3 SCN registers [cite: 226]

[cite\_start]These registers are accessible using slave address 200. [cite: 227]

| Description | Start Address | End Address | Num of Registers | R/W | Data Type | Step | Additional Info |
| :--- | :--- | :--- | :--- | :-: | :--- | :--- | :--- |
| **SCN name** | 1400 | 1403 | 4 | R | STRING | n.a. | |
| **SCN Sockets** | 1404 | 1404 | 1 | R | UNSIGNED16 | 1A | [cite\_start]Number of configured sockets [cite: 223] |
| **SCN Total Consumption Phase L1** | 1405 | 1406 | 2 | R | FLOAT32 | 1A | |
| **SCN Total Consumption Phase L2** | 1407 | 1408 | 2 | R | FLOAT32 | 1A | |
| **SCN Total Consumption Phase L3** | 1409 | 1410 | 2 | R | FLOAT32 | 1A | |
| **SCN Actual Max Current Phase L1** | 1411 | 1412 | 2 | R | FLOAT32 | 1A | |
| **SCN Actual Max Current Phase L2** | 1413 | 1414 | 2 | R | FLOAT32 | 1A | |
| **SCN Actual Max Current Phase L3** | 1415 | 1416 | 2 | R | FLOAT32 | 1A | |
| **SCN Max Current per Phase L1** | 1417 | 1418 | 2 | R/W | FLOAT32 | 1A | |
| **SCN Max Current per Phase L2** | 1419 | 1420 | 2 | R/W | FLOAT32 | 1A | |
| **SCN Max Current per Phase L3** | 1421 | 1422 | 2 | R/W | FLOAT32 | 1A | |
| **Remaining valid time Max Current Phase L1** | 1423 | 1424 | 2 | R | UNSIGNED32 | 1s | [cite\_start]Max current valid time [cite: 223] |
| **Remaining valid time Max Current Phase L2** | 1425 | 1426 | 2 | R | UNSIGNED32 | 1s | [cite\_start]Max current valid time [cite: 223] |
| **Remaining valid time Max Current Phase L3** | 1427 | 1428 | 2 | R | UNSIGNED32 | 1s | [cite\_start]Max current valid time [cite: 223] |
| **SCN Safe current** | 1429 | 1430 | 2 | R | FLOAT32 | 1A | [cite\_start]Configured SCN safe current [cite: 223] |
| **SCN Modbus Slave Max Current enable** | 1431 | 1431 | 1 | R | UNSIGNED16 | n.a. | [cite\_start]1:Enabled, 0: Disabled. [cite: 223] |

### [cite\_start]3.4 Socket measurement registers [cite: 233]

[cite\_start]These registers show information from the energy meter. [cite: 234] [cite\_start]For a single-socket station, they relate to the only socket. [cite: 234] [cite\_start]For a dual-socket station, they relate to the left socket. [cite: 234] [cite\_start]They are accessible via slave address 1. [cite: 234] [cite\_start]For a dual-socket station, the right socket's measurements are accessible via slave address 2. [cite: 234]

| Description | Start Address | End Address | Num of Registers | R/W | Data Type | Step | Additional Info |
| :--- | :--- | :--- | :--- | :-: | :--- | :--- | :--- |
| **Meter state** | 300 | 300 | 1 | R | UNSIGNED16 | n.a. | [cite\_start]Bitmask with state:\<br\>Initialised: $0\\times01$\<br\>Updated: $0\\times02$\<br\>Warning: $0\\times04$\<br\>Error: $0\\times08$ [cite: 230] |
| **Meter last value timestamp** | 301 | 304 | 4 | R | UNSIGNED64 | 0.001s | [cite\_start]Milliseconds since last received measurement [cite: 230] |
| **Meter type** | 305 | 305 | 1 | R | UNSIGNED16 | n.a. | [cite\_start]0:RTU, 1:TCP/IP, 2:UDP, 3:P1, 4:other [cite: 230] |
| **Voltage Phase V(L1-N)** | 306 | 307 | 2 | R | FLOAT32 | 1V | |
| **Voltage Phase V(L2-N)** | 308 | 309 | 2 | R | FLOAT32 | 1V | |
| **Voltage Phase V(L3-N)** | 310 | 311 | 2 | R | FLOAT32 | 1V | |
| **Voltage Phase V(L1-L2)** | 312 | 313 | 2 | R | FLOAT32 | 1V | |
| **Voltage Phase V(L2-L3)** | 314 | 315 | 2 | R | FLOAT32 | 1V | |
| **Voltage Phase $V(L3-L1)$** | 316 | 317 | 2 | R | FLOAT32 | 1V | |
| **Current N** | 318 | 319 | 2 | R | FLOAT32 | 1A | |
| **Current Phase L1** | 320 | 321 | 2 | R | FLOAT32 | 1A | |
| **Current Phase L2** | 322 | 323 | 2 | R | FLOAT32 | 1A | |
| **Current Phase L3** | 324 | 325 | 2 | R | FLOAT32 | 1A | |
| **Current Sum** | 326 | 327 | 2 | R | FLOAT32 | 1A | |
| **Power Factor Phase L1** | 328 | 329 | 2 | R | FLOAT32 | N.A. | |
| **Power Factor Phase L2** | 330 | 331 | 2 | R | FLOAT32 | N.A. | |
| **Power Factor Phase L3** | 332 | 333 | 2 | R | FLOAT32 | N.A. | |
| **Power Factor Sum** | 334 | 335 | 2 | R | FLOAT32 | N.A. | |
| **Frequency** | 336 | 337 | 2 | R | FLOAT32 | 1Hz | |
| **Real Power Phase L1** | 338 | 339 | 2 | R | FLOAT32 | 1W | |
| **Real Power Phase L2** | 340 | 341 | 2 | R | FLOAT32 | 1W | |
| **Real Power Phase L3** | 342 | 343 | 2 | R | FLOAT32 | 1W | |
| **Real Power Sum** | 344 | 345 | 2 | R | FLOAT32 | 1W | |
| **Apparent Power Phase L1** | 346 | 347 | 2 | R | FLOAT32 | 1VA | |
| **Apparent Power Phase L2** | 348 | 349 | 2 | R | FLOAT32 | 1VA | |
| **Apparent Power Phase L3** | 350 | 351 | 2 | R | FLOAT32 | 1VA | |
| **Apparent Power Sum** | 352 | 353 | 2 | R | FLOAT32 | 1VA | |
| **Reactive Power Phase L1** | 354 | 355 | 2 | R | FLOAT32 | 1VAr | |
| **Reactive Power Phase L2** | 356 | 357 | 2 | R | FLOAT32 | 1VAr | |
| **Reactive Power Phase L3** | 358 | 359 | 2 | R | FLOAT32 | 1VAr | |
| **Reactive Power Sum** | 360 | 361 | 2 | R | FLOAT32 | 1VAr | |
| **Real Energy Delivered Phase L1** | 362 | 365 | 4 | R | FLOAT64 | 1Wh | |
| **Real Energy Delivered Phase L2** | 366 | 369 | 4 | R | FLOAT64 | 1Wh | |
| **Real Energy Delivered Phase L3** | 370 | 373 | 4 | R | FLOAT64 | 1Wh | |
| **Real Energy Delivered Sum** | 374 | 377 | 4 | R | FLOAT64 | 1Wh | |
| **Real Energy Consumed Phase L1** | 378 | 381 | 4 | R | FLOAT64 | 1Wh | |
| **Real Energy Consumed Phase L2** | 382 | 385 | 4 | R | FLOAT64 | 1Wh | |
| **Real Energy Consumed Phase L3** | 386 | 389 | 4 | R | FLOAT64 | 1Wh | |
| **Real Energy Consumed Sum** | 390 | 393 | 4 | R | FLOAT64 | 1Wh | |
| **Apparent Energy Phase L1** | 394 | 397 | 4 | R | FLOAT64 | 1VAh | |
| **Apparent Energy Phase L2** | 398 | 401 | 4 | R | FLOAT64 | 1VAh | |
| **Apparent Energy Phase L3** | 402 | 405 | 4 | R | FLOAT64 | 1VAh | |
| **Apparent Energy Sum** | 406 | 409 | 4 | R | FLOAT64 | 1VAh | |
| **Reactive Energy Phase L1** | 410 | 413 | 4 | R | FLOAT64 | 1VArh | |
| **Reactive Energy Phase L2** | 414 | 417 | 4 | R | FLOAT64 | 1VArh | |
| **Reactive Energy Phase L3** | 418 | 421 | 4 | R | FLOAT64 | 1VArh | |
| **Reactive Energy Sum** | 422 | 425 | 4 | R | FLOAT64 | 1VArh | |
| **Availability** | 1200 | 1200 | 1 | R | UNSIGNED16 | n.a. | [cite\_start]1: Operative, 0: inoperative [cite: 239] |
| **Mode 3 state** | 1201 | 1205 | 5 | R | STRING | n.a. | [cite\_start]61851 states [cite: 239] |
| **Actual Applied Max Current** | 1206 | 1207 | 2 | R | FLOAT32 | 1A | [cite\_start]Actual Applied overall Max Current for socket [cite: 239] |
| **Modbus Slave Max Current valid time** | 1208 | 1209 | 2 | R | UNSIGNED32 | 1s | [cite\_start]Remaining time before fall back to safe current [cite: 239] |
| **Modbus Slave Max Current** | 1210 | 1211 | 2 | R/W | FLOAT32 | 1A | |
| **Active Load Balancing Safe Current** | 1212 | 1213 | 2 | R | FLOAT32 | 1A | [cite\_start]Active Load Balancing safe current [cite: 239] |
| **Modbus Slave received setpoint accounted for** | 1214 | 1214 | 1 | R | UNSIGNED16 | n.a. | [cite\_start]1:Yes, 0: No [cite: 239] |
| **Charge using 1 or 3 phases** | 1215 | 1215 | 1 | R/W | UNSIGNED16 | phases | [cite\_start]1: 1 phase, 3: 3 phase charging [cite: 239] |

[cite\_start]**Note:** Register 1214, 'Modbus Slave received setpoint accounted for', shows whether the Max Current setpoint received (registers 1210-1211) is being used to determine the 'Actual Applied Max Current' (registers 1206-1207). [cite: 240, 245] [cite\_start]Depending on other setpoints, registers 1206-1207 might read the settings sent by the external device (e.g., an EMS). [cite: 245]

### [cite\_start]3.4.1 Mode 3 state listing [cite: 246]

| State | Signal voltage (DC) | PWM signal applied | Vehicle connected | Charging |
| :--- | :--- | :--- | :--- | :--- |
| **A** | 12V | No | No | [cite\_start]No [cite: 247] |
| **B1** | 9V | No | Yes | [cite\_start]No [cite: 247] |
| **B2** | 9V | Yes | Yes | [cite\_start]No [cite: 247] |
| **C1** | 6V | No | Yes | [cite\_start]No [cite: 247] |
| **C2** | 6V | Yes | Yes | [cite\_start]Yes [cite: 247] |
| **D1** | 3V | No | Yes | [cite\_start]No [cite: 247] |
| **D2** | 3V | Yes | Yes | [cite\_start]Yes [cite: 247] |
| **E** | 0V | No | No | [cite\_start]No [cite: 247] |
| **F** | -12V | No | No | [cite\_start]No [cite: 247] |

[cite\_start]Note that State F indicates an error state. [cite: 248]
