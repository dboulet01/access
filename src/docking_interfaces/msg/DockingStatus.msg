uint8 HOLD=0
uint8 APPROACH=1
uint8 FINAL_APPROACH=2
uint8 SOFT_CAPTURE=3
uint8 HARD_DOCK=4
uint8 ABORTED=255

builtin_interfaces/Time stamp
uint8 state
float64 range_m
bool transition_pending
string detail