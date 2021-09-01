const InterfaceFFI = require("./ffiInterface");
const utf8 = require("utf8");

class TransportType {
  #tari_transport_type_ptr;
  #type = "None";

  pointerAssign(ptr, type) {
    // Prevent pointer from being leaked in case of re-assignment
    if (this.#tari_transport_type_ptr) {
      this.destroy();
      this.#tari_transport_type_ptr = ptr;
      this.#type = type;
    } else {
      this.#tari_transport_type_ptr = ptr;
      this.#type = type;
    }
  }

  getPtr() {
    return this.#tari_transport_type_ptr;
  }

  getType() {
    return this.#type;
  }

  static createMemory() {
    let result = new TransportType();
    result.pointerAssign(InterfaceFFI.transportMemoryCreate(), "Memory");
    return result;
  }

  static createTCP(listener_address) {
    let sanitize = utf8.encode(listener_address); // Make sure it's not UTF-16 encoded (JS default)
    let result = new TransportType();
    result.pointerAssign(InterfaceFFI.transportTcpCreate(sanitize), "TCP");
    return result;
  }

  static createTor(
    control_server_address,
    tor_cookie,
    tor_port,
    socks_username,
    socks_password
  ) {
    let sanitize_address = utf8.encode(control_server_address);
    let sanitize_username = utf8.encode(socks_username);
    let sanitize_password = utf8.encode(socks_password);
    let result = new TransportType();
    result.pointerAssign(
      InterfaceFFI.transportTorCreate(
        sanitize_address,
        tor_cookie.getPtr(),
        tor_port,
        sanitize_username,
        sanitize_password
      ),
      "Tor"
    );
    return result;
  }

  getAddress() {
    if (this.#type === "Memory") {
      let c_address = InterfaceFFI.transportMemoryGetAddress(this.getPtr());
      let result = c_address.readCString();
      InterfaceFFI.stringDestroy(c_address);
      return result;
    } else {
      return "N/A";
    }
  }

  destroy() {
    this.#type = "None";
    if (this.#tari_transport_type_ptr) {
      InterfaceFFI.transportTypeDestroy(this.#tari_transport_type_ptr);
      this.#tari_transport_type_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = TransportType;
