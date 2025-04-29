use self::traits::{OrderBookIndex, OrderInterface};

use super::{Order as OrderUnit, *};
use codec::DecodeWithMemTracking;

#[derive(Encode, Decode, DecodeWithMemTracking, Debug, Default, Clone, PartialEq, Eq, TypeInfo)]
pub struct CritbitTree<K, V> {
    /// Index of the root node which is part of the internal nodes.
    root: K,
    /// The internal nodes of the tree
    /// Here, `key` refers to `index` of `InternalNode.
    internal_nodes: BTreeMap<K, InternalNode<K>>,
    /// The leaf nodes of the tree.
    leaves: BTreeMap<K, LeafNode<K, V>>,
    /// Index of the largest value of the leaf nodes. Could be updated for every insertion.
    max_leaf_index: K,
    /// Index of the smallest value of the leaf nodes. Could be updated for every insertion.
    min_leaf_index: K,
    /// Index of the next internal node which should be incremented for every insertion.
    next_internal_node_index: K,
    /// Index of the next leaf node which should be incremented for every insertion.
    next_leaf_node_index: K,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, TypeInfo)]
pub enum NodeKind {
    /// The node is an interior node.
    Internal,
    /// The node is a leaf node.
    #[default]
    Leaf,
}

impl<K, V> CritbitTree<K, V>
where
    K: OrderBookIndex,
    V: Clone + PartialOrd,
{
    /// Create new instance of the tree.
    pub fn new() -> Self {
        Self {
            root: K::PARTITION_INDEX,
            internal_nodes: BTreeMap::new(),
            leaves: BTreeMap::new(),
            max_leaf_index: K::PARTITION_INDEX,
            min_leaf_index: K::PARTITION_INDEX,
            next_internal_node_index: Default::default(),
            next_leaf_node_index: Default::default(),
        }
    }

    /// Check whether the leaf exists
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Get the number of leaf nodes in the tree.
    pub fn size(&self) -> usize {
        self.leaves.len()
    }

    /// Query the maximum leaf node. Return **(key, index)**.
    ///
    /// Index indicates the index of the leaf node which is encoded as `(K::MAX_INDEX - tree_index)`
    pub fn max_leaf(&self) -> Result<Option<(K, K)>, CritbitTreeError> {
        if self.leaves.is_empty() {
            return Ok(None);
        }
        if let Some(leaf) = self.leaves.get(&self.max_leaf_index) {
            Ok(Some((leaf.key(), self.max_leaf_index)))
        } else {
            // For safety, should not reach here unless leaves are empty
            Err(CritbitTreeError::LeafNodeShouldExist)
        }
    }

    /// Query the minimum leaf node. Return **(key, index)**.
    ///
    /// Index indicates the index of the leaf node which is encoded as `(K::MAX_INDEX - tree_index)`
    pub fn min_leaf(&self) -> Result<Option<(K, K)>, CritbitTreeError> {
        if self.leaves.is_empty() {
            return Ok(None);
        }
        if let Some(leaf) = self.leaves.get(&self.min_leaf_index) {
            Ok(Some((leaf.key(), self.min_leaf_index)))
        } else {
            // For safety, should not reach here unless leaves are empty
            Err(CritbitTreeError::LeafNodeShouldExist)
        }
    }

    /// Insert a new leaf node into the tree for given key `K` and value `V`
    pub fn insert(&mut self, key: K, value: V) -> Result<(), CritbitTreeError> {
        let new_leaf = LeafNode::new(key, value);
        let new_leaf_index = self.next_index(NodeKind::Leaf)?;
        if let Some(_) = self.leaves.insert(new_leaf_index, new_leaf) {
            return Err(CritbitTreeError::UniqueIndex);
        }
        let closest_leaf_index = self.get_closest_leaf_index(&key)?;
        if closest_leaf_index == None {
            // Handle first insertion
            self.root = K::MAX_INDEX;
            self.max_leaf_index = new_leaf_index;
            self.min_leaf_index = new_leaf_index;
            return Ok(());
        }
        let closest_leaf_key = self
            .leaves
            .get(
                &closest_leaf_index.expect("Case for `None` is already handled"), // TODO: Safe way
            )
            .ok_or(CritbitTreeError::LeafNodeShouldExist)?
            .key;
        if closest_leaf_key == key {
            return Err(CritbitTreeError::AlreadyExist);
        }
        let new_mask = K::new_mask(&key, &closest_leaf_key);
        let new_internal_node = InternalNode::new(new_mask);
        let new_internal_index = self.next_index(NodeKind::Internal)?;
        if let Some(_) = self
            .internal_nodes
            .insert(new_internal_index, new_internal_node.clone())
        {
            return Err(CritbitTreeError::UniqueIndex);
        }
        let mut curr = self.root;
        let mut internal_node_parent_index = K::PARTITION_INDEX;
        while curr < K::PARTITION_INDEX {
            let internal_node = self
                .internal_nodes
                .get(&curr)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?;
            if new_mask > internal_node.mask {
                break;
            }
            internal_node_parent_index = curr;
            if internal_node.mask & key == Zero::zero() {
                curr = internal_node.left;
            } else {
                curr = internal_node.right;
            }
        }
        if internal_node_parent_index.is_partition_index() {
            // If the new internal node is the root
            self.root = new_internal_index;
        } else {
            // Update child for the parent internal node
            let is_left_child = self.is_left_child(&internal_node_parent_index, &curr);
            self.update_ref(
                internal_node_parent_index,
                new_internal_index,
                is_left_child,
            )?;
        }
        // Update child for new internal node
        let is_left_child = key & new_internal_node.mask == Zero::zero();
        self.update_ref(
            new_internal_index,
            K::MAX_INDEX - new_leaf_index,
            is_left_child,
        )?;
        self.update_ref(new_internal_index, curr, !is_left_child)?;

        // Update min/max leaf
        self.update_min_max_leaf(new_leaf_index, key);
        Ok(())
    }

    /// Remove leaf for given `index`.
    ///
    /// **Index here indicates the index of the leaves**
    pub fn remove_leaf_by_index(&mut self, leaf_index: &K) -> Result<V, CritbitTreeError> {
        let leaf_node = self
            .leaves
            .get(leaf_index)
            .ok_or(CritbitTreeError::NotFound)?;
        // Update min/max leaf index
        let mut is_empty: bool = false;
        if &self.min_leaf_index == leaf_index {
            if let Some((_, next_leaf_index)) = self.next_leaf(&leaf_node.key)? {
                self.min_leaf_index = next_leaf_index;
            } else {
                is_empty = true;
            }
        }
        if &self.max_leaf_index == leaf_index {
            if let Some((_, prev_leaf_index)) = self.previous_leaf(&leaf_node.key)? {
                self.max_leaf_index = prev_leaf_index;
            } else {
                is_empty = true;
            }
        }
        let LeafNode {
            value,
            parent: parent_index,
            ..
        } = self
            .leaves
            .remove(leaf_index)
            .expect("We already check above");
        if is_empty {
            self.reset()
        } else {
            let parent_node = self
                .internal_nodes
                .get(&parent_index)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?;
            // Sibling node could be internal node or leaf
            let sibling_node_index =
                if self.is_left_child(&parent_index, &(K::MAX_INDEX - *leaf_index)) {
                    parent_node.right
                } else {
                    parent_node.left
                };
            let grand_parent_index = parent_node.parent;
            if grand_parent_index == K::PARTITION_INDEX {
                // Removed parent is root node
                if sibling_node_index < K::PARTITION_INDEX {
                    let mut sibling_node = self
                        .internal_nodes
                        .get_mut(&sibling_node_index)
                        .ok_or(CritbitTreeError::InternalNodeShouldExist)?
                        .clone();
                    sibling_node.parent = K::PARTITION_INDEX;
                    self.internal_nodes.insert(sibling_node_index, sibling_node);
                } else {
                    let mut sibling_node = self
                        .leaves
                        .get_mut(&(K::MAX_INDEX - sibling_node_index))
                        .ok_or(CritbitTreeError::LeafNodeShouldExist)?
                        .clone();
                    sibling_node.parent = K::PARTITION_INDEX;
                    self.leaves
                        .insert(K::MAX_INDEX - sibling_node_index, sibling_node);
                }
                self.root = sibling_node_index;
            } else {
                // Removed parent of grandparent is internal node
                let is_left_child = self.is_left_child(&grand_parent_index, &parent_index);
                self.update_ref(grand_parent_index, sibling_node_index, is_left_child)?;
            }
            self.internal_nodes.remove(&parent_index);
        }

        Ok(value)
    }

    /// Reset the tree.
    fn reset(&mut self) {
        self.root = K::PARTITION_INDEX;
        self.min_leaf_index = K::PARTITION_INDEX;
        self.max_leaf_index = K::PARTITION_INDEX;
        self.next_internal_node_index = Zero::zero();
        self.next_leaf_node_index = Zero::zero();
    }

    /// Find previous leaf for given key `K`. Return **(key, index)** where `index` indicates leaf
    /// index which is encoded as `K::MAX_INDEX - tree_index` Return `None` if leaf node for given
    /// key is the minimum leaf node
    ///
    /// **Key indicates the key of the leaf node.**
    pub fn previous_leaf(&self, key: &K) -> Result<Option<(K, K)>, CritbitTreeError> {
        let leaf_index = self.find_leaf(key)?.ok_or(CritbitTreeError::NotFound)?;
        let mut parent_node_index = self
            .leaves
            .get(&leaf_index)
            .ok_or(CritbitTreeError::LeafNodeShouldExist)?
            .parent;
        let mut ptr = K::MAX_INDEX - leaf_index;
        while parent_node_index != K::PARTITION_INDEX
            && self.is_left_child(&parent_node_index, &ptr)
        {
            ptr = parent_node_index;
            parent_node_index = self
                .internal_nodes
                .get(&ptr)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?
                .parent;
        }
        // This means previous leaf doesn't exist
        if parent_node_index == K::PARTITION_INDEX {
            return Ok(None);
        }
        let start_index = self
            .internal_nodes
            .get(&parent_node_index)
            .ok_or(CritbitTreeError::InternalNodeShouldExist)?
            .left;
        let leaf_index = K::MAX_INDEX - self.right_most_leaf(start_index)?;
        let leaf_node = self
            .leaves
            .get(&leaf_index)
            .ok_or(CritbitTreeError::LeafNodeShouldExist)?;

        Ok(Some((leaf_node.clone().key, leaf_index)))
    }

    /// Find next leaf for given key `K`. Return **(key, index)** where `index` indicates leaf index
    /// which is encoded as `K::MAX_INDEX - tree_index` Return `None` if leaf node for given key is
    /// the maximum leaf node
    ///
    /// **Key indicates the key of the leaf node.**
    pub fn next_leaf(&self, key: &K) -> Result<Option<(K, K)>, CritbitTreeError> {
        let leaf_index = self.find_leaf(key)?.ok_or(CritbitTreeError::NotFound)?;
        let mut parent_node_index = self
            .leaves
            .get(&leaf_index)
            .ok_or(CritbitTreeError::LeafNodeShouldExist)?
            .parent;
        let mut ptr = K::MAX_INDEX - leaf_index;
        while parent_node_index != K::PARTITION_INDEX
            && !self.is_left_child(&parent_node_index, &ptr)
        {
            ptr = parent_node_index;
            parent_node_index = self
                .internal_nodes
                .get(&ptr)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?
                .parent;
        }
        // This means next leaf doesn't exist
        if parent_node_index == K::PARTITION_INDEX {
            return Ok(None);
        }
        let start_index = self
            .internal_nodes
            .get(&parent_node_index)
            .ok_or(CritbitTreeError::InternalNodeShouldExist)?
            .right;
        let leaf_index = K::MAX_INDEX - self.left_most_leaf(&start_index)?;
        let leaf_node = self
            .leaves
            .get(&leaf_index)
            .ok_or(CritbitTreeError::LeafNodeShouldExist)?;

        Ok(Some((leaf_node.clone().key, leaf_index)))
    }

    /// Get the left most leaf index of the tree. Index indicates the index inside the tree.
    ///
    /// Should not return `Error` unless the tree is not empty.
    fn left_most_leaf(&self, root: &K) -> Result<K, CritbitTreeError> {
        let mut curr = root.clone();
        while curr < K::PARTITION_INDEX {
            let internal_node = self
                .internal_nodes
                .get(&curr)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?;
            curr = internal_node.left;
        }
        Ok(curr)
    }

    /// Get the right most leaf index of the tree. Index indicates the index inside the tree.
    ///
    /// Should not return `Error` unless the tree is not empty.
    fn right_most_leaf(&self, root: K) -> Result<K, CritbitTreeError> {
        let mut curr = root.clone();
        if curr == K::PARTITION_INDEX {
            return Err(CritbitTreeError::NotInitialized);
        }
        while curr < K::PARTITION_INDEX {
            let internal_node = self
                .internal_nodes
                .get(&curr)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?;
            curr = internal_node.right;
        }
        Ok(curr)
    }

    /// Find leaf index for given `key`.
    /// Return
    ///
    /// - `leaf_index` which indicates the index inside the leaf which is encoded as `K::MAX_INDEX -
    ///   leaf_index`.
    /// - `K::PARTITION_INDEX`, if tree is empty
    /// - `None`, if key doesn't match with closest key
    pub fn find_leaf(&self, key: &K) -> Result<Option<K>, CritbitTreeError> {
        if let Some(leaf_index) = self.get_closest_leaf_index(key)? {
            let leaf_node = self
                .leaves
                .get(&leaf_index)
                .ok_or(CritbitTreeError::LeafNodeShouldExist)?;
            if &leaf_node.key != key {
                Ok(None)
            } else {
                Ok(Some(leaf_index))
            }
        } else {
            // Tree is empty
            Ok(None)
        }
    }

    /// Update the minimum and maximum leaf nodes.
    fn update_min_max_leaf(&mut self, new_leaf_index: K, new_key: K) {
        if let Some(min_leaf_value) = self.leaves.get(&self.min_leaf_index) {
            if min_leaf_value.key > new_key {
                self.min_leaf_index = new_leaf_index;
            }
        } else {
            self.min_leaf_index = new_leaf_index;
        }
        if let Some(max_leaf_value) = self.leaves.get(&self.max_leaf_index) {
            if max_leaf_value.key < new_key {
                self.max_leaf_index = new_leaf_index;
            }
        } else {
            self.max_leaf_index = new_leaf_index;
        }
    }

    /// Check if the index is the left child of the parent. Index indicates index of the tree
    fn is_left_child(&self, parent: &K, child: &K) -> bool {
        if let Some(internal_node) = self.internal_nodes.get(parent) {
            return &internal_node.left == child;
        }
        // Should not reach here
        false
    }

    /// Get the next index based on `NodeKind`, which maybe leaf or internal for the tree.
    fn next_index(&mut self, kind: NodeKind) -> Result<K, CritbitTreeError> {
        let index = match kind {
            NodeKind::Leaf => {
                let index = self.next_leaf_node_index;
                self.next_leaf_node_index += One::one();
                ensure!(
                    self.next_leaf_node_index <= K::CAPACITY,
                    CritbitTreeError::ExceedCapacity
                );
                index
            }
            NodeKind::Internal => {
                let index = self.next_internal_node_index;
                self.next_internal_node_index = self
                    .next_internal_node_index
                    .checked_add(&One::one())
                    .ok_or(CritbitTreeError::Overflow)?;
                index
            }
        };
        Ok(index)
    }

    /// Update the tree reference which could be 'leaf' or 'internal' node.
    /// Both parent and child are tree index
    fn update_ref(
        &mut self,
        parent: K,
        child: K,
        is_left_child: bool,
    ) -> Result<(), CritbitTreeError> {
        let mut internal_node = self
            .internal_nodes
            .get(&parent)
            .ok_or(CritbitTreeError::InternalNodeShouldExist)?
            .clone();
        if is_left_child {
            internal_node.left = child;
        } else {
            internal_node.right = child;
        }
        self.internal_nodes.insert(parent, internal_node);
        if child > K::PARTITION_INDEX {
            // child is `leaf`
            let leaf_node_index = K::MAX_INDEX - child;
            let mut leaf_node = self
                .leaves
                .get(&leaf_node_index)
                .ok_or(CritbitTreeError::LeafNodeShouldExist)?
                .clone();
            leaf_node.parent = parent;
            self.leaves.insert(leaf_node_index, leaf_node);
        } else {
            // child is `internal_node`
            let mut internal_node = self
                .internal_nodes
                .get(&child)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?
                .clone();
            internal_node.parent = parent;
            self.internal_nodes.insert(child, internal_node);
        }
        Ok(())
    }

    /// Get the closest leaf index which encoded as `K::MAX_INDEX - tree_index` to the given key.
    fn get_closest_leaf_index(&self, key: &K) -> Result<Option<K>, CritbitTreeError> {
        let mut curr = self.root;
        if curr == K::PARTITION_INDEX {
            // Case: Tree is empty(e.g first insertion)
            return Ok(None);
        }
        while curr < K::PARTITION_INDEX {
            let internal_node = self
                .internal_nodes
                .get(&curr)
                .ok_or(CritbitTreeError::InternalNodeShouldExist)?;
            if internal_node.mask & *key == Zero::zero() {
                curr = internal_node.left;
            } else {
                curr = internal_node.right;
            }
        }

        Ok(Some(K::MAX_INDEX - curr))
    }
}

#[derive(Debug, PartialEq)]
pub enum CritbitTreeError {
    /// The number of leaf nodes exceeds the capacity of the tree.
    ExceedCapacity,
    /// The index overflows the maximum index of the tree.
    Overflow,
    /// The index is already in use.
    UniqueIndex,
    /// The key already exists in the tree.
    AlreadyExist,
    /// Error for safe code
    InternalNodeShouldExist,
    /// Error for safe code
    LeafNodeShouldExist,
    /// `Leaf` or `Internal Node` may not exist for given index
    NotFound,
    /// Error on remove which may be caused by empty tree after remove
    RemoveNotAllowed,
    /// Error on operations on valued
    ValueOps,
    /// Error on `tree
    NotInitialized,
}

/// `InternalNode` for `critbit-tree` with `K` index. Here, `K` refer to two meaning.
/// -  mask
/// - path of the tree
#[derive(Encode, Decode, DecodeWithMemTracking, Debug, Default, Clone, PartialEq, Eq, TypeInfo)]
pub struct InternalNode<K> {
    /// Mask for branching the tree based on the critbit.
    mask: K,
    /// Parent index of the node.
    parent: K,
    /// Left child index of the node.
    left: K,
    /// Right child index of the node.
    right: K,
}

impl<K: OrderBookIndex> InternalNode<K> {
    /// Create new instance of the interior node.
    pub fn new(mask: K) -> Self {
        InternalNode {
            mask,
            parent: K::PARTITION_INDEX,
            left: K::PARTITION_INDEX,
            right: K::PARTITION_INDEX,
        }
    }
}

/// Type of `LeafNode`
///
/// - `parent`: Index of the path of the tree
/// - `key`: Value represents the node(e.g price)
/// - `value`: Actual data stored on the node(e.g orders)
#[derive(Encode, Decode, DecodeWithMemTracking, Debug, Default, Clone, PartialEq, Eq, TypeInfo)]
pub struct LeafNode<K, V> {
    /// Parent index of the node.
    parent: K,
    /// Key of the node.
    key: K,
    /// Value of the node.
    value: V,
}

impl<K: OrderBookIndex, V> LeafNode<K, V> {
    /// Return clone of key of `self`
    pub fn key(&self) -> K {
        self.key.clone()
    }

    /// Create new instance of the leaf node with given `key` and `value`.
    /// `parent` is initialized to `K::PARTITION_INDEX` which refer to the start index of the leaf node
    pub fn new(key: K, value: V) -> Self {
        LeafNode {
            parent: K::PARTITION_INDEX,
            key,
            value,
        }
    }
}

impl<Account, Unit, Order, BlockNumber> OrderBook<Account, Unit, BlockNumber>
    for CritbitTree<Unit, Order>
where
    Account: Clone,
    Unit: OrderBookIndex,
    Order: OrderInterface<Account, Unit, BlockNumber> + Clone + PartialOrd,
{
    type Order = Order;
    type OrderId = <Order as OrderInterface<Account, Unit, BlockNumber>>::OrderId;
    type Error = CritbitTreeError;

    fn new() -> Self {
        CritbitTree::new()
    }

    fn size(&self) -> usize {
        self.size()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn get_orders(&self, owner: &Account) -> Vec<OrderUnit<Unit, Account, BlockNumber>> {
        let mut res = Vec::new();
        self.leaves
            .values()
            .for_each(|l| match l.value.find_order_of(owner) {
                Some(mut orders) => res.append(&mut orders),
                None => {}
            });
        res
    }

    fn open_orders_at(&self, key: Unit) -> Result<Option<Self::Order>, Self::Error> {
        if let Some(leaf_index) = self.find_leaf(&key)? {
            if let Some(leaf) = self.leaves.get(&leaf_index) {
                Ok(Some(leaf.value.orders()))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn min_order(&self) -> Option<(Unit, Unit)> {
        if let Ok(maybe_min) = self.min_leaf() {
            maybe_min
        } else {
            // Tree is empty
            None
        }
    }

    fn max_order(&self) -> Option<(Unit, Unit)> {
        if let Ok(maybe_max) = self.max_leaf() {
            maybe_max
        } else {
            // Tree is empty
            None
        }
    }

    fn place_order(
        &mut self,
        order_id: Self::OrderId,
        owner: &Account,
        key: Unit,
        quantity: Unit,
        expired_at: BlockNumber,
    ) -> Result<(), Self::Error> {
        if let Some(leaf_index) = self.find_leaf(&key)? {
            if leaf_index == Unit::PARTITION_INDEX {
                // Should not reach here
                return Err(CritbitTreeError::LeafNodeShouldExist);
            }
            let leaf_node = self
                .leaves
                .get_mut(&leaf_index)
                .ok_or(CritbitTreeError::LeafNodeShouldExist)?;
            leaf_node
                .value
                .placed(order_id, owner, quantity, expired_at);
            Ok(())
        } else {
            // Insert new leaf node with new order
            self.insert(
                key,
                Order::new(order_id, owner.clone(), quantity, expired_at),
            )?;
            Ok(())
        }
    }

    fn fill_order(
        &mut self,
        key: Unit,
        quantity: Unit,
    ) -> Result<Option<Vec<(Account, Unit)>>, Self::Error> {
        let maybe_filled = if let Some(leaf_index) = self.find_leaf(&key)? {
            if let Some(leaf) = self.leaves.get_mut(&leaf_index) {
                let filled = leaf.value.filled(quantity);
                if leaf.value.is_empty() {
                    self.remove_leaf_by_index(&leaf_index)?;
                }
                filled
            } else {
                None
            }
        } else {
            // If there is no leaf for the given price(key), orders will not be filled
            None
        };

        Ok(maybe_filled)
    }

    fn cancel_order(
        &mut self,
        maybe_owner: &Account,
        key: Unit,
        order_id: Self::OrderId,
        quantity: Unit,
    ) -> Result<(), Self::Error> {
        if let Some(leaf_index) = self.find_leaf(&key)? {
            if leaf_index == Unit::PARTITION_INDEX {
                // Since tree is empty, we don't do anything
                return Ok(());
            }
            if let Some(leaf) = self.leaves.get_mut(&leaf_index) {
                leaf.value
                    .canceled(maybe_owner, order_id, quantity)
                    .map_err(|_| CritbitTreeError::ValueOps)?;
                Ok(())
            } else {
                // no leaf?
                return Err(CritbitTreeError::NotFound);
            }
        } else {
            Err(CritbitTreeError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critbit_tree() {
        assert_eq!(u64::new_mask(&0, &1).leading_zeros(), 63)
    }

    #[test]
    fn insert_works() {
        let mut tree = CritbitTree::<u64, u64>::new();

        // Check whether the tree is empty
        assert_eq!(tree.min_leaf().unwrap(), None);
        assert_eq!(tree.max_leaf().unwrap(), None);
        assert!(tree.is_empty());
        assert_eq!(tree.size(), 0);

        // First insertion
        let k1 = 0x1000u64;
        let v = 0u64;
        tree.insert(k1, v).unwrap();

        // Check validity of first insertion of tree
        assert_eq!(tree.size(), 1);
        assert_eq!(tree.root, u64::MAX);
        assert_eq!(tree.min_leaf().unwrap(), Some((k1, 0)));
        assert_eq!(tree.max_leaf().unwrap(), Some((k1, 0)));

        let previous_leaf = tree.previous_leaf(&k1).unwrap();
        let next_leaf = tree.next_leaf(&k1).unwrap();
        assert_eq!(previous_leaf, None);
        assert_eq!(next_leaf, None);
        println!("First Insertion => {:?}", tree);

        // Second insertion
        let k2 = 0x100u64;
        let v = 0u64;
        tree.insert(k2, v).unwrap();
        assert_eq!(tree.size(), 2);

        // Check whether root index has updated
        assert_eq!(tree.root, 0u64);

        // Check min & max upated
        assert_eq!(tree.min_leaf().unwrap(), Some((k2, 1)));
        assert_eq!(tree.max_leaf().unwrap(), Some((k1, 0)));

        // Check whether `find_leaf` works
        let leaf_index = tree.find_leaf(&k2);
        assert_eq!(leaf_index.unwrap(), Some(1));

        assert_eq!(tree.previous_leaf(&k2).unwrap(), None);
        assert_eq!(tree.previous_leaf(&k1).unwrap(), Some((k2, 1)));
        assert_eq!(tree.next_leaf(&k2).unwrap(), Some((k1, 0)));

        println!("{:?}", tree);
    }

    #[test]
    fn remove_works() {
        let mut tree = CritbitTree::<u64, u64>::new();
        // first insertion
        let k_v: (u64, u64) = (0x1u64, 0);
        tree.insert(k_v.0, k_v.1).unwrap();
        assert_eq!(tree.remove_leaf_by_index(&0), Ok(0));
        assert_eq!(tree.size(), 0);
        assert_eq!(tree.is_empty(), true);
        println!("Empty tree => {:?}", tree);
        println!("--------------------------");
        let k_v: Vec<(u64, u64)> = vec![(0x2u64, 1), (0x3u64, 2), (0x4u64, 3), (0x5u64, 4)];
        for (k, v) in k_v.clone() {
            tree.insert(k, v).unwrap();
        }
        assert_eq!(tree.size(), k_v.len());
        println!("After insert 4 items => {:?}", tree);
        println!("--------------------------");
        tree.remove_leaf_by_index(&0).unwrap();
        assert_eq!(tree.min_leaf().unwrap(), Some((0x3u64, 1)));
        assert_eq!(tree.size(), k_v.len() - 1);
        println!("Delete left most leaf => {:?}", tree);
    }
}
