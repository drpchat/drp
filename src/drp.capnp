@0xceb71d1333e035f1;

struct Message {
    union {
        register :group {
            name @0 :Data;
        }

        send :group {
            dest @1 :Data;
            body @2 :Data;
        }

        relay :group {
            source @3 :Data;
            dest @4 :Data;
            body @5 :Data;
        }

        join :group {
            channel @6 :Data;
        }

        part :group {
            channel @7 :Data;
        }
    }
}
