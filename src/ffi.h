#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// Status of the VPN connection
enum class VpnStatus {
  Disconnected = 0,
  Connecting = 1,
  Connected = 2,
  Error = 3,
};

template<typename T = void>
struct Arc;

struct SamlServer;

struct VpnApp;

/// Opaque handle to the VPN client
struct VpnClientHandle {
  Arc<VpnApp> vpn_app;
  SamlServer saml_server;
  void (*callback)(VpnStatus status, void *user_data);
  void *callback_data;
};

/// Configuration for VPN connection
struct VpnConfig {
  const char *config_path;
  const char *server_address;
  unsigned int port;
};

extern "C" {

/// Creates a new VPN client instance
VpnClientHandle *openaws_vpn_client_new();

/// Sets a status change callback
void openaws_vpn_client_set_status_callback(VpnClientHandle *client,
                                            void (*callback)(VpnStatus status, void *user_data),
                                            void *user_data);

/// Sets the VPN configuration
int openaws_vpn_client_set_config(VpnClientHandle *client, VpnConfig config);

/// Gets the current status of the VPN connection
VpnStatus openaws_vpn_client_get_status(const VpnClientHandle *client);

/// Get the URL for SAML authentication
int openaws_vpn_client_get_saml_url(VpnClientHandle *client, char **out_url, char **out_password);

/// Free a string allocated by the library
void openaws_vpn_client_free_string(char *string);

/// Set up the SAML server
int openaws_vpn_client_start_saml_server(VpnClientHandle *client);

/// Connects to the VPN using SAML authentication
int openaws_vpn_client_connect_saml(VpnClientHandle *client,
                                    const char *saml_response,
                                    const char *saml_password);

/// Disconnects from the VPN
int openaws_vpn_client_disconnect(VpnClientHandle *client);

/// Frees resources used by the VPN client
void openaws_vpn_client_free(VpnClientHandle *client);

} // extern "C"
