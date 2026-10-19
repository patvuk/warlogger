import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "patvuk.warlogger"
  ipcTarget: "patvuk.warlogger"

  readonly property string runtimeDir: Quickshell.env("XDG_RUNTIME_DIR") || "/tmp"
  readonly property string statusPath: runtimeDir + "/warlogger/status.json"
  readonly property string configPath: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/warlogger/config"
  readonly property string terminal: "omarchy-launch-floating-terminal-with-presentation "
  readonly property string glyph: "󰓕"

  readonly property var games: [
    { id: "wow", label: "WoW" },
    { id: "classic", label: "Classic" },
    { id: "fresh", label: "Anniversary" },
    { id: "sod", label: "SoD" },
    { id: "vanilla", label: "Era" },
    { id: "titan", label: "Titan" },
    { id: "ff", label: "FFXIV" },
    { id: "eso", label: "ESO" },
    { id: "swtor", label: "SWTOR" },
    { id: "fellowship", label: "Fellowship" }
  ]

  property var status: null
  property bool alive: false
  property string game: "wow"
  property string folder: ""
  property var logs: []

  readonly property string phase: status ? status.state : "idle"
  readonly property bool busy: alive && (phase === "working" || phase === "live")
  readonly property bool live: alive && phase === "live"
  readonly property string report: status ? status.report : ""
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  function gameLabel() {
    for (var i = 0; i < root.games.length; i++) {
      if (root.games[i].id === root.game) return root.games[i].label
    }
    return root.game
  }

  function barText() {
    if (root.live) return root.glyph + " LIVE"
    if (root.busy) return root.glyph + " " + Math.round(root.status.progress * 100) + "%"
    return root.glyph
  }

  function stateLabel() {
    if (root.live) return "Live logging"
    if (root.busy) return "Uploading"
    if (root.phase === "error") return "Failed"
    if (root.phase === "done") return "Uploaded"
    return "Idle"
  }

  function parseStatus(text) {
    try {
      root.status = JSON.parse(text)
      proc.path = "/proc/" + root.status.pid + "/stat"
      proc.reload()
    } catch (e) {
      root.status = null
      root.alive = false
    }
  }

  function parseConfig(text) {
    var game = /^\s*game\s*=\s*(\S+)/m.exec(text)
    root.game = game ? game[1] : "wow"
  }

  function parseLogs(text) {
    var lines = text.split("\n")
    var rows = []
    root.folder = lines.length ? lines[0] : ""
    for (var i = 1; i < lines.length && rows.length < 8; i++) {
      var m = /^\s*(\d+)\s+([\d.]+ MB)\s+(\d+) min ago\s+(.+)$/.exec(lines[i])
      if (m) rows.push({ index: m[1], size: m[2], age: ageLabel(parseInt(m[3])), name: m[4] })
    }
    root.logs = rows
  }

  function ageLabel(minutes) {
    if (minutes < 60) return minutes + " min"
    if (minutes < 1440) return Math.round(minutes / 60) + " h"
    return Math.round(minutes / 1440) + " d"
  }

  function run(command) {
    root.close()
    if (root.bar) root.bar.run(command)
  }

  function exec(command) {
    if (root.bar) root.bar.run(command)
  }

  function selectGame(id) {
    setGame.command = ["warlogger", "set", "game", id]
    setGame.running = true
  }

  function refreshLogs() {
    if (!list.running) list.running = true
  }

  onOpenedChanged: if (opened) { config.reload(); root.refreshLogs() }
  Component.onCompleted: config.reload()

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  FileView {
    id: file
    path: root.statusPath
    watchChanges: false
    printErrors: false
    onLoaded: root.parseStatus(text())
    onLoadFailed: { root.status = null; root.alive = false }
  }

  FileView {
    id: proc
    watchChanges: false
    printErrors: false
    onLoaded: root.alive = true
    onLoadFailed: root.alive = false
  }

  FileView {
    id: config
    path: root.configPath
    watchChanges: false
    printErrors: false
    onLoaded: root.parseConfig(text())
  }

  Process {
    id: list
    command: ["warlogger", "logs"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.parseLogs(text)
    }
  }

  Process {
    id: setGame
    onExited: {
      config.reload()
      root.refreshLogs()
    }
  }

  Timer {
    interval: Math.max(1, root.setting("refreshIntervalSec", 2)) * 1000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: file.reload()
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.bar && root.bar.vertical ? root.glyph : root.barText()
    active: root.busy || root.phase === "error"
    dimmed: !root.busy && root.phase !== "done"
    tooltipText: root.opened ? "" : "Warlogger\n" + root.gameLabel() + " · " + root.phaseLabel()
    onPressed: function(b) { root.toggle() }
  }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(400))
    contentHeight: panel.fittedContentHeight(column.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Column {
        id: column
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        spacing: Style.space(14)

        Item {
          width: parent.width
          implicitHeight: Math.max(heroIcon.implicitHeight, heroLabels.implicitHeight, heroValue.implicitHeight)

          Text {
            id: heroIcon
            text: root.glyph
            color: root.phase === "error" ? (root.bar ? root.bar.urgent : Color.urgent) : root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.display
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
          }

          Column {
            id: heroLabels
            anchors.left: heroIcon.right
            anchors.leftMargin: Style.space(14)
            anchors.right: heroValue.left
            anchors.rightMargin: Style.space(10)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(2)

            Text {
              text: root.gameLabel()
              color: root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.title
              font.bold: true
              elide: Text.ElideRight
              width: parent.width
            }

            Text {
              text: root.phaseLabel().toUpperCase()
              color: Qt.darker(root.foreground, 1.4)
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              font.bold: true
              font.letterSpacing: 1.2
              elide: Text.ElideRight
              width: parent.width
            }
          }

          Text {
            id: heroValue
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: root.busy
            text: root.live ? "LIVE" : (root.status ? Math.round(root.status.progress * 100) + "%" : "")
            color: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.displayLarge
            font.bold: true
          }
        }

        InfoLabel {
          visible: root.status !== null && root.status.text !== ""
          width: parent.width
          wrapMode: Text.Wrap
          text: root.status ? root.status.text : ""
        }

        PanelSeparator { foreground: root.foreground }

        PanelSectionHeader {
          text: "GAME"
          foreground: root.foreground
          fontFamily: root.fontFamily
        }

        Flow {
          width: parent.width
          spacing: Style.space(6)

          Repeater {
            model: root.games

            Button {
              required property var modelData
              text: (modelData.id === root.game ? "● " : "") + modelData.label
              fontSize: Style.font.bodySmall
              foreground: root.foreground
              fontFamily: root.fontFamily
              horizontalPadding: Style.spacing.controlPaddingX
              verticalPadding: Style.spacing.controlPaddingY
              bordered: true
              onClicked: root.selectGame(modelData.id)
            }
          }
        }

        PanelSeparator { foreground: root.foreground }

        PanelSectionHeader {
          text: "LOGS"
          foreground: root.foreground
          fontFamily: root.fontFamily
        }

        InfoLabel {
          visible: root.logs.length === 0
          width: parent.width
          wrapMode: Text.Wrap
          text: root.folder ? "No logs in " + root.folder : "No log folder found; run warlogger set dir." + root.game + " <path>"
        }

        Column {
          width: parent.width
          spacing: Style.space(4)

          Repeater {
            model: root.logs

            LogRow {
              required property var modelData
              row: modelData
            }
          }
        }

        PanelSeparator { foreground: root.foreground }

        Grid {
          id: actions
          width: parent.width
          columns: 3
          spacing: Style.space(6)

          readonly property real cellWidth: (width - spacing * (columns - 1)) / columns

          ActionButton {
            iconText: "󰅒"
            text: "Upload latest"
            enabled: !root.busy
            onClicked: root.exec("warlogger upload latest")
          }

          ActionButton {
            iconText: root.live ? "󰓛" : "󰐊"
            text: root.live ? "Stop live" : "Live"
            enabled: root.live || !root.busy
            onClicked: root.exec(root.live ? "warlogger stop" : "warlogger live")
          }

          ActionButton {
            iconText: "󰎬"
            text: "Report"
            enabled: root.report !== ""
            onClicked: root.run("xdg-open " + root.report)
          }

          ActionButton {
            iconText: "󰀄"
            text: "Login"
            onClicked: root.run(root.terminal + "warlogger login")
          }

          ActionButton {
            iconText: "󰉋"
            text: "Folder"
            enabled: root.folder !== ""
            onClicked: root.run("xdg-open '" + root.folder + "'")
          }

          ActionButton {
            iconText: "󰋼"
            text: "About"
            onClicked: root.run("xdg-open https://github.com/patvuk/warlogger")
          }
        }
      }
    }
  }

  component ActionButton: Button {
    width: actions.cellWidth
    iconSize: Style.font.title
    fontSize: Style.font.bodySmall
    foreground: root.foreground
    fontFamily: root.fontFamily
    horizontalPadding: Style.spacing.controlPaddingX
    verticalPadding: Style.spacing.controlPaddingY + Style.space(2)
    bordered: true
  }

  component LogRow: Item {
    id: logRow

    property var row: ({})

    width: parent.width
    implicitHeight: logName.implicitHeight + Style.space(8)

    Rectangle {
      anchors.fill: parent
      radius: Style.space(4)
      color: Qt.rgba(root.foreground.r, root.foreground.g, root.foreground.b, hover.containsMouse ? 0.08 : 0)
    }

    InfoValue {
      id: logName
      anchors.left: parent.left
      anchors.leftMargin: Style.space(6)
      anchors.right: logMeta.left
      anchors.rightMargin: Style.space(8)
      anchors.verticalCenter: parent.verticalCenter
      elide: Text.ElideMiddle
      text: logRow.row.name
    }

    InfoLabel {
      id: logMeta
      anchors.right: parent.right
      anchors.rightMargin: Style.space(6)
      anchors.verticalCenter: parent.verticalCenter
      text: logRow.row.size + " · " + logRow.row.age
    }

    MouseArea {
      id: hover
      anchors.fill: parent
      hoverEnabled: true
      enabled: !root.busy
      cursorShape: Qt.PointingHandCursor
      onClicked: root.exec("warlogger upload " + logRow.row.index)
    }
  }

  component InfoLabel: Text {
    textFormat: Text.PlainText
    color: root.foreground
    opacity: 0.6
    font.family: root.fontFamily
    font.pixelSize: Style.font.bodySmall
  }

  component InfoValue: Text {
    textFormat: Text.PlainText
    color: root.foreground
    font.family: root.fontFamily
    font.pixelSize: Style.font.bodySmall
  }
}
