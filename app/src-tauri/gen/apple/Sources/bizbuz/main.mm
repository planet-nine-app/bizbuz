#include "bindings/bindings.h"

extern "C" void BizbuzQuickActionsBridgeInit(void);

int main(int argc, char * argv[]) {
	BizbuzQuickActionsBridgeInit();
	ffi::start_app();
	return 0;
}
