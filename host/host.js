var window = globalThis;
var self = globalThis;
var __out = [];
var __timers = [];
var __listeners = [];

var __sink = { postMessage: function (m) { __out.push(m); } };
window.parent = __sink;
window.top = __sink;
window.opener = null;

window.addEventListener = function (type, fn) { if (type === "message") __listeners.push(fn); };
window.removeEventListener = function () {};
window.dispatchEvent = function () { return true; };
window.postMessage = function () {};
window.sendToHost = function (channel, id, event, data) { __out.push({ message: channel, id: id, data: data }); };
window.setWarningText = function () {};
window.setErrorText = function () {};
window.sendLogMessage = function () {};
window.sendEventMessage = function () {};

window.setTimeout = function (fn) { if (typeof fn === "function") __timers.push(fn); return __timers.length; };
window.setInterval = function () { return 0; };
window.clearTimeout = function () {};
window.clearInterval = function () {};
window.requestAnimationFrame = window.setTimeout;
window.queueMicrotask = window.queueMicrotask || window.setTimeout;

var __node = function () {
  return {
    style: {}, dataset: {}, children: [], classList: { add: function () {}, remove: function () {} },
    appendChild: function (c) { return c; }, removeChild: function () {}, setAttribute: function () {},
    addEventListener: function () {}, removeEventListener: function () {}, insertBefore: function (c) { return c; },
  };
};

window.document = {
  readyState: "complete", body: __node(), head: __node(), documentElement: __node(),
  createElement: __node, createTextNode: __node,
  getElementById: function () { return null; },
  getElementsByTagName: function () { return [__node()]; },
  querySelector: function () { return null; },
  querySelectorAll: function () { return []; },
  addEventListener: function () {}, removeEventListener: function () {},
};

window.location = {
  search: "?id=1&gameContentDetectionEnabled=false&metersEnabled=false&liveFightDataEnabled=false&gameVersionId=",
  href: "about:blank", origin: "null", protocol: "about:", host: "", hostname: "", pathname: "/",
};

window.navigator = { userAgent: "warlogger", platform: "", language: "en-US", languages: ["en-US"] };
window.performance = window.performance || { now: function () { return Date.now(); } };
window.localStorage = { getItem: function () { return null; }, setItem: function () {}, removeItem: function () {} };
window.sessionStorage = window.localStorage;
window.fetch = function () { return Promise.reject(new Error("offline")); };
window.console = window.console || { log: function () {}, warn: function () {}, error: function () {}, info: function () {}, debug: function () {} };

if (typeof URLSearchParams === "undefined") {
  window.URLSearchParams = function (init) {
    var map = {};
    String(init || "").replace(/^\?/, "").split("&").forEach(function (pair) {
      if (!pair) return;
      var kv = pair.split("=");
      map[decodeURIComponent(kv[0])] = decodeURIComponent(kv.slice(1).join("="));
    });
    this.get = function (k) { return Object.prototype.hasOwnProperty.call(map, k) ? map[k] : null; };
    this.has = function (k) { return Object.prototype.hasOwnProperty.call(map, k); };
    this.set = function (k, v) { map[k] = String(v); };
  };
}

function __drain() {
  for (var guard = 0; __timers.length && guard < 10000; guard++) {
    var batch = __timers.splice(0, __timers.length);
    for (var i = 0; i < batch.length; i++) {
      try { batch[i](); } catch (e) {}
    }
  }
}

function __send(msg) {
  var event = { data: msg, origin: "*", source: __sink, ports: [] };
  for (var i = 0; i < __listeners.length; i++) __listeners[i](event);
  __drain();
}

var __last = {};

function __take(name) {
  for (var i = 0; i < __out.length; i++) {
    if (__out[i] && __out[i].message === name) {
      __last[name] = __out[i];
      __out.length = 0;
      return true;
    }
  }
  return false;
}

function __version() {
  var r = __last["get-parser-version-completed"];
  return Number(r.data !== undefined ? r.data : r.parserVersion) || 0;
}

function __parseError() {
  var r = __last["parse-lines-completed"];
  return r.success === false ? "line " + r.parsedLineCount + ": " + r.exception + " - " + r.line : "";
}

function __fights() {
  return __last["collect-fights-completed"] || { fights: [] };
}

function __summary() {
  var f = __fights();
  return JSON.stringify({ count: f.fights.length, start: f.startTime || 0, end: f.endTime || 0, mythic: f.mythic || 0 });
}

function __fightsText() {
  var f = __fights(), n = 0, s = "";
  for (var i = 0; i < f.fights.length; i++) {
    n += f.fights[i].eventCount;
    s += f.fights[i].eventsString;
  }
  return f.logVersion + "|" + f.gameVersion + "\n" + n + "\n" + s;
}

function __masterError() {
  var m = __last["collect-master-info-completed"];
  return m.success ? "" : "master table belongs to " + m.actualReportCode + ", expected " + m.expectedReportCode;
}

function __masterText() {
  var m = __last["collect-master-info-completed"], f = __fights();
  var details = f.logFileDetails === undefined ? "" : f.logFileDetails;
  return f.logVersion + "|" + f.gameVersion + "|" + details + "\n" +
    m.lastAssignedActorID + "\n" + m.actorsString +
    m.lastAssignedAbilityID + "\n" + m.abilitiesString +
    m.lastAssignedTupleID + "\n" + m.tuplesString +
    m.lastAssignedPetID + "\n" + m.petsString;
}

function __players() {
  return String(__last["collect-master-info-completed"].playersString);
}

function __scanned() {
  return JSON.stringify(Array.prototype.slice.call(__last["collect-scanned-raids-completed"]));
}

function __release() {
  __last = {};
}
