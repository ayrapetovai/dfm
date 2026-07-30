13. Currently in the state file there are fields in each registration record: secs_since_epoch and nanos_since_epoch. They ocupy too much space, rename them to uts (unix time seconds) and utn (unix time nanos), the renaming in mapped struct in rust code is unessasery, just map thoese fields to the shorter names.

