@0xceb71d1333e035f1;

struct Message {
    union {
        register :group {
            name @0 :Data;
            pubkey @12 :Data;
        }

        send :group {
            dest @1 :Data;
            body @2 :Data;
            nonce @13 :Data;
        }


        join :group {
            channel @6 :Data;
        }

        part :group {
            channel @7 :Data;
        }


        whois :group {
            name @9 :Data;
        }

        theyare :group {
            name @10 :Data;
            pubkey @11 :Data;
        }


        response :group {
            body @8 :Data;
        }

        relay :group {
            source @3 :Data;
            dest @4 :Data;
            body @5 :Data;
        }
    }
}
