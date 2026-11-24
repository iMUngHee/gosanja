package flow

import "C"

func CGOOnly() int { return int(C.int(7)) }
