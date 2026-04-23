use std::collections::HashMap;

use serde_json::Value;

/// Map from imp id to a stored auction response body (JSON).
pub type ImpsWithBidResponses = HashMap<String, Value>;

/// Map from imp id to (bidder name -> stored bid response body).
pub type ImpBidderStoredResp = HashMap<String, HashMap<String, Value>>;

/// Map from imp id to (bidder name -> replace-imp-id flag).
pub type ImpBidderReplaceImpId = HashMap<String, HashMap<String, bool>>;
