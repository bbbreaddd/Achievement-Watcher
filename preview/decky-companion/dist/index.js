const manifest = {"name":"Achievement Watcher"};
const API_VERSION = 2;
const internalAPIConnection = window.__DECKY_SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED_deckyLoaderAPIInit;
if (!internalAPIConnection) {
    throw new Error('[@decky/api]: Failed to connect to the loader as as the loader API was not initialized. This is likely a bug in Decky Loader.');
}
let api;
try {
    api = internalAPIConnection.connect(API_VERSION, manifest.name);
}
catch {
    api = internalAPIConnection.connect(1, manifest.name);
    console.warn(`[@decky/api] Requested API version ${API_VERSION} but the running loader only supports version 1. Some features may not work.`);
}
if (api._version != API_VERSION) {
    console.warn(`[@decky/api] Requested API version ${API_VERSION} but the running loader only supports version ${api._version}. Some features may not work.`);
}
const routerHook = api.routerHook;
const definePlugin = (fn) => {
    return (...args) => {
        return fn(...args);
    };
};

var DefaultContext = {
  color: undefined,
  size: undefined,
  className: undefined,
  style: undefined,
  attr: undefined
};
var IconContext = SP_REACT.createContext && /*#__PURE__*/SP_REACT.createContext(DefaultContext);

var _excluded = ["attr", "size", "title"];
function _objectWithoutProperties(e, t) { if (null == e) return {}; var o, r, i = _objectWithoutPropertiesLoose(e, t); if (Object.getOwnPropertySymbols) { var n = Object.getOwnPropertySymbols(e); for (r = 0; r < n.length; r++) o = n[r], -1 === t.indexOf(o) && {}.propertyIsEnumerable.call(e, o) && (i[o] = e[o]); } return i; }
function _objectWithoutPropertiesLoose(r, e) { if (null == r) return {}; var t = {}; for (var n in r) if ({}.hasOwnProperty.call(r, n)) { if (-1 !== e.indexOf(n)) continue; t[n] = r[n]; } return t; }
function _extends() { return _extends = Object.assign ? Object.assign.bind() : function (n) { for (var e = 1; e < arguments.length; e++) { var t = arguments[e]; for (var r in t) ({}).hasOwnProperty.call(t, r) && (n[r] = t[r]); } return n; }, _extends.apply(null, arguments); }
function ownKeys(e, r) { var t = Object.keys(e); if (Object.getOwnPropertySymbols) { var o = Object.getOwnPropertySymbols(e); r && (o = o.filter(function (r) { return Object.getOwnPropertyDescriptor(e, r).enumerable; })), t.push.apply(t, o); } return t; }
function _objectSpread(e) { for (var r = 1; r < arguments.length; r++) { var t = null != arguments[r] ? arguments[r] : {}; r % 2 ? ownKeys(Object(t), true).forEach(function (r) { _defineProperty(e, r, t[r]); }) : Object.getOwnPropertyDescriptors ? Object.defineProperties(e, Object.getOwnPropertyDescriptors(t)) : ownKeys(Object(t)).forEach(function (r) { Object.defineProperty(e, r, Object.getOwnPropertyDescriptor(t, r)); }); } return e; }
function _defineProperty(e, r, t) { return (r = _toPropertyKey(r)) in e ? Object.defineProperty(e, r, { value: t, enumerable: true, configurable: true, writable: true }) : e[r] = t, e; }
function _toPropertyKey(t) { var i = _toPrimitive(t, "string"); return "symbol" == typeof i ? i : i + ""; }
function _toPrimitive(t, r) { if ("object" != typeof t || !t) return t; var e = t[Symbol.toPrimitive]; if (void 0 !== e) { var i = e.call(t, r); if ("object" != typeof i) return i; throw new TypeError("@@toPrimitive must return a primitive value."); } return ("string" === r ? String : Number)(t); }
function Tree2Element(tree) {
  return tree && tree.map((node, i) => /*#__PURE__*/SP_REACT.createElement(node.tag, _objectSpread({
    key: i
  }, node.attr), Tree2Element(node.child)));
}
function GenIcon(data) {
  return props => /*#__PURE__*/SP_REACT.createElement(IconBase, _extends({
    attr: _objectSpread({}, data.attr)
  }, props), Tree2Element(data.child));
}
function IconBase(props) {
  var elem = conf => {
    var attr = props.attr,
      size = props.size,
      title = props.title,
      svgProps = _objectWithoutProperties(props, _excluded);
    var computedSize = size || conf.size || "1em";
    var className;
    if (conf.className) className = conf.className;
    if (props.className) className = (className ? className + " " : "") + props.className;
    return /*#__PURE__*/SP_REACT.createElement("svg", _extends({
      stroke: "currentColor",
      fill: "currentColor",
      strokeWidth: "0"
    }, conf.attr, attr, svgProps, {
      className: className,
      style: _objectSpread(_objectSpread({
        color: props.color || conf.color
      }, conf.style), props.style),
      height: computedSize,
      width: computedSize,
      xmlns: "http://www.w3.org/2000/svg"
    }), title && /*#__PURE__*/SP_REACT.createElement("title", null, title), props.children);
  };
  return IconContext !== undefined ? /*#__PURE__*/SP_REACT.createElement(IconContext.Consumer, null, conf => elem(conf)) : elem(DefaultContext);
}

// THIS FILE IS AUTO GENERATED
function FaTrophy (props) {
  return GenIcon({"attr":{"viewBox":"0 0 576 512"},"child":[{"tag":"path","attr":{"d":"M552 64H448V24c0-13.3-10.7-24-24-24H152c-13.3 0-24 10.7-24 24v40H24C10.7 64 0 74.7 0 88v56c0 35.7 22.5 72.4 61.9 100.7 31.5 22.7 69.8 37.1 110 41.7C203.3 338.5 240 360 240 360v72h-48c-35.3 0-64 20.7-64 56v12c0 6.6 5.4 12 12 12h296c6.6 0 12-5.4 12-12v-12c0-35.3-28.7-56-64-56h-48v-72s36.7-21.5 68.1-73.6c40.3-4.6 78.6-19 110-41.7 39.3-28.3 61.9-65 61.9-100.7V88c0-13.3-10.7-24-24-24zM99.3 192.8C74.9 175.2 64 155.6 64 144v-16h64.2c1 32.6 5.8 61.2 12.8 86.2-15.1-5.2-29.2-12.4-41.7-21.4zM512 144c0 16.1-17.7 36.1-35.3 48.8-12.5 9-26.7 16.2-41.8 21.4 7-25 11.8-53.6 12.8-86.2H512v16z"},"child":[]}]})(props);
}

const APP_ROUTE = '/library/app/:appid';
function GamePageProbe() {
    const appId = window.location.pathname.match(/\/library\/app\/(\d+)/)?.[1];
    return (SP_JSX.jsxs("aside", { style: {
            position: 'fixed',
            right: 24,
            bottom: 24,
            zIndex: 1000,
            padding: '12px 16px',
            background: '#17212b',
            borderLeft: '4px solid #66c0f4',
            color: '#f5f5f5',
            boxShadow: '0 4px 16px rgba(0, 0, 0, 0.35)',
            pointerEvents: 'none',
        }, children: [SP_JSX.jsx("div", { style: { fontSize: 16, fontWeight: 600 }, children: "Achievement Watcher" }), SP_JSX.jsxs("div", { style: { marginTop: 3, color: '#acb2b8', fontSize: 12 }, children: ["Game page connection working", appId ? ` · App ${appId}` : ''] })] }));
}
function patchAppPage() {
    const routePatch = routerHook.addPatch(APP_ROUTE, (props) => ({
        ...props,
        children: (SP_JSX.jsxs(SP_JSX.Fragment, { children: [props.children, SP_JSX.jsx(GamePageProbe, {})] })),
    }));
    return () => routerHook.removePatch(APP_ROUTE, routePatch);
}

function Content() {
    const [connected, setConnected] = SP_REACT.useState(false);
    const [latest, setLatest] = SP_REACT.useState();
    SP_REACT.useEffect(() => {
        let socket;
        let retry;
        let disposed = false;
        const connect = () => {
            socket = new WebSocket('ws://127.0.0.1:8082');
            socket.onopen = () => setConnected(true);
            socket.onmessage = ({ data }) => {
                try {
                    setLatest(JSON.parse(String(data)));
                }
                catch {
                    // Ignore messages from unrelated local WebSocket services.
                }
            };
            socket.onclose = () => {
                setConnected(false);
                if (!disposed)
                    retry = window.setTimeout(connect, 3000);
            };
        };
        connect();
        return () => {
            disposed = true;
            window.clearTimeout(retry);
            socket?.close();
        };
    }, []);
    return (SP_JSX.jsxs(DFL.PanelSection, { title: "Companion status", children: [SP_JSX.jsx(DFL.PanelSectionRow, { children: SP_JSX.jsx("div", { children: connected ? 'Connected to Achievement Watcher' : 'Waiting for Achievement Watcher' }) }), latest && (SP_JSX.jsx(DFL.PanelSectionRow, { children: SP_JSX.jsxs("div", { children: [SP_JSX.jsx("strong", { children: latest.displayName ?? 'Achievement unlocked' }), SP_JSX.jsx("div", { children: latest.game }), latest.description && SP_JSX.jsx("small", { children: latest.description })] }) }))] }));
}
var index = definePlugin(() => {
    const removeAppPagePatch = patchAppPage();
    return {
        name: 'Achievement Watcher',
        titleView: SP_JSX.jsx("div", { className: DFL.staticClasses.Title, children: "Achievement Watcher" }),
        content: SP_JSX.jsx(Content, {}),
        icon: SP_JSX.jsx(FaTrophy, {}),
        onDismount: removeAppPagePatch,
    };
});

export { index as default };
//# sourceMappingURL=index.js.map
