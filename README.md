### Reconnaissance
The reconnaissance repo leverages the power of reth and its modular design to get mass network exposure with very little overhead
while from the outside functioning as a healthy node.

Just like in real life, we can get faulty information from doing reconnaissance. in our case, we don't validate our transactions
rather we delegate this to our local full node for validation, if we get a bad transaction we will remove the peer that shared it. 
otherwise we will forward it