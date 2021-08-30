const InterfaceFFI = require("./ffiInterface");
const utf8 = require("utf8");

class CommsConfig {
  #comms_config_ptr;

  constructor(
    public_address,
    transport_ptr,
    database_name,
    datastore_path,
    discovery_timeout_in_secs,
    saf_message_duration_in_secs,
    network
  ) {
    let sanitize_address = utf8.encode(public_address);
    let sanitize_db_name = utf8.encode(database_name);
    let sanitize_db_path = utf8.encode(datastore_path);
    let sanitize_network = utf8.encode(network);
    this.#comms_config_ptr = InterfaceFFI.commsConfigCreate(
      sanitize_address,
      transport_ptr,
      sanitize_db_name,
      sanitize_db_path,
      discovery_timeout_in_secs,
      saf_message_duration_in_secs,
      sanitize_network
    );
  }

  getPtr() {
    return this.#comms_config_ptr;
  }

  destroy() {
    if (this.#comms_config_ptr) {
      InterfaceFFI.commsConfigDestroy(this.#comms_config_ptr);
      this.#comms_config_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = CommsConfig;
