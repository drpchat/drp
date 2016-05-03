@0xceb71d1333e035f1;

struct Message {
    union {
        register :group {
            name @0 :Text;
            pubkey @12 :Data;
        }

        send :group {
            dest @1 :Data;
            body @2 :Data;
            union {
                nonce @13 :Data;
                unencrypted @14 :Void;
            }
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
            union {
                nonce @15 :Data;
                unencrypted @16 :Void;
            }
        }
    }
}
